//! Call expression checking (overload resolution, argument collection, signature instantiation).
//!
//! Decomposed by responsibility:
//! - `applicability`: Adapter for solver call/new resolution.
//! - `candidate_collection`: Argument type collection with contextual typing and spread expansion.
//! - `diagnostics`: Diagnostic filtering/rollback helpers for speculative call checking.
//! - `overload_resolution`: Overload resolution across multiple signatures.

mod applicability;
mod candidate_collection;
mod diagnostics;
mod overload_resolution;

use crate::query_boundaries::common::AssignabilityChecker;
use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_solver::TypeId;

/// Call-local context carrying the callable type during argument collection.
///
/// Replaces the ambient `ctx.current_callable_type` field. Threaded explicitly
/// through `collect_call_argument_types_with_context` and its transitive callees
/// so that rest-parameter position checks (TS2556) and generic excess-property
/// skip decisions can query the callable shape without ambient mutable state.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CallableContext {
    /// The callable type of the call expression being processed.
    pub callable_type: Option<TypeId>,
}

impl CallableContext {
    pub const fn new(callable_type: TypeId) -> Self {
        Self {
            callable_type: Some(callable_type),
        }
    }

    pub const fn none() -> Self {
        Self {
            callable_type: None,
        }
    }
}

pub(crate) struct OverloadResolution {
    pub(crate) arg_types: Vec<TypeId>,
    pub(crate) result: CallResult,
}

pub(super) struct CheckerCallAssignabilityAdapter<'s, 'ctx> {
    pub(super) state: &'s mut CheckerState<'ctx>,
}

impl AssignabilityChecker for CheckerCallAssignabilityAdapter<'_, '_> {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.state.is_assignable_to(source, target)
    }
    fn is_assignable_to_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        self.state.is_assignable_to_strict(source, target)
    }

    fn is_assignable_to_bivariant_callback(&mut self, source: TypeId, target: TypeId) -> bool {
        self.state.is_assignable_to_bivariant(source, target)
    }

    fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        self.state.evaluate_type_for_assignability(type_id)
    }

    fn expand_type_alias_application(&mut self, type_id: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
        use crate::query_boundaries::state::type_environment::application_info;

        let (base, args) = application_info(self.state.ctx.types, type_id)?;
        let sym_id = self.state.ctx.resolve_type_to_symbol_id(base)?;
        let (body, type_params) = self.state.type_reference_symbol_type_with_params(sym_id);
        if body == TypeId::ANY || body == TypeId::ERROR || type_params.is_empty() {
            return None;
        }
        let subst = TypeSubstitution::from_args(self.state.ctx.types, &type_params, &args);
        let instantiated = instantiate_type(self.state.ctx.types, body, &subst);
        if instantiated == type_id {
            None
        } else {
            Some(instantiated)
        }
    }

    fn promise_like_type_argument(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.state
            .promise_like_return_type_argument(type_id)
            .or_else(|| {
                let resolved = self.state.resolve_lazy_type(type_id);
                (resolved != type_id)
                    .then(|| self.state.promise_like_return_type_argument(resolved))
                    .flatten()
            })
    }

    fn type_resolver(&self) -> Option<&dyn tsz_solver::TypeResolver> {
        Some(&self.state.ctx)
    }

    fn are_types_identical(&mut self, a: TypeId, b: TypeId) -> bool {
        if a == b {
            return true;
        }
        let a_resolved = self.state.resolve_lazy_type(a);
        let b_resolved = self.state.resolve_lazy_type(b);
        if a_resolved == b_resolved {
            return true;
        }
        self.state.ensure_relation_input_ready(a_resolved);
        self.state.ensure_relation_input_ready(b_resolved);
        self.state.is_assignable_to(a_resolved, b_resolved)
            && self.state.is_assignable_to(b_resolved, a_resolved)
    }
}
