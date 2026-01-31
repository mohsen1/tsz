//! Generic Type Argument Checking Module
//!
//! This module contains methods for validating generic type arguments.
//! It handles:
//! - Type argument constraint validation (TS2344)
//! - Call expression type argument validation
//! - New expression type argument validation
//!
//! This module extends CheckerState with generic-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::solver::TypeId;

// =============================================================================
// Generic Type Argument Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Argument Validation
    // =========================================================================

    /// Validate explicit type arguments against their constraints for call expressions.
    /// Reports TS2344 when a type argument doesn't satisfy its constraint.
    pub(crate) fn validate_call_type_arguments(
        &mut self,
        callee_type: TypeId,
        type_args_list: &crate::parser::NodeList,
        _call_idx: NodeIndex,
    ) {
        use crate::solver::AssignabilityChecker;
        use crate::solver::type_queries::{
            TypeArgumentExtractionKind, classify_for_type_argument_extraction,
        };

        // Get the type parameters from the callee type
        let type_params = match classify_for_type_argument_extraction(self.ctx.types, callee_type) {
            TypeArgumentExtractionKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                shape.type_params.clone()
            }
            TypeArgumentExtractionKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                // For callable types, use the first signature's type params
                shape
                    .call_signatures
                    .first()
                    .map(|sig| sig.type_params.clone())
                    .unwrap_or_default()
            }
            TypeArgumentExtractionKind::Other => return,
        };

        if type_params.is_empty() {
            return;
        }

        // Collect the provided type arguments
        let type_args: Vec<TypeId> = type_args_list
            .nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();

        for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
            if let Some(constraint) = param.constraint {
                // Instantiate the constraint with already-validated type arguments
                let instantiated_constraint = if i > 0 {
                    let mut subst = crate::solver::TypeSubstitution::new();
                    for (j, p) in type_params.iter().take(i).enumerate() {
                        if let Some(&arg) = type_args.get(j) {
                            subst.insert(p.name, arg);
                        }
                    }
                    crate::solver::instantiate_type(self.ctx.types, constraint, &subst)
                } else {
                    constraint
                };

                let is_satisfied = {
                    let env = self.ctx.type_env.borrow();
                    let mut checker =
                        crate::solver::CompatChecker::with_resolver(self.ctx.types, &*env);
                    self.ctx.configure_compat_checker(&mut checker);
                    checker.is_assignable_to(type_arg, instantiated_constraint)
                };

                if !is_satisfied {
                    // Report TS2344 at the specific type argument node
                    if let Some(&arg_idx) = type_args_list.nodes.get(i) {
                        self.error_type_constraint_not_satisfied(
                            type_arg,
                            instantiated_constraint,
                            arg_idx,
                        );
                    }
                }
            }
        }
    }

    /// Validate explicit type arguments against their constraints for new expressions.
    /// Reports TS2344 when a type argument doesn't satisfy its constraint.
    pub(crate) fn validate_new_expression_type_arguments(
        &mut self,
        constructor_type: TypeId,
        type_args_list: &crate::parser::NodeList,
        _call_idx: NodeIndex,
    ) {
        use crate::solver::AssignabilityChecker;
        use crate::solver::type_queries::get_callable_shape;

        // Get the type parameters from the constructor type
        let Some(shape) = get_callable_shape(self.ctx.types, constructor_type) else {
            return;
        };
        // For callable types, use the first construct signature's type params
        let type_params = shape
            .construct_signatures
            .first()
            .map(|sig| sig.type_params.clone())
            .unwrap_or_default();

        if type_params.is_empty() {
            return;
        }

        // Collect the provided type arguments
        let type_args: Vec<TypeId> = type_args_list
            .nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();

        for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
            if let Some(constraint) = param.constraint {
                // Instantiate the constraint with already-validated type arguments
                let instantiated_constraint = if i > 0 {
                    let mut subst = crate::solver::TypeSubstitution::new();
                    for (j, p) in type_params.iter().take(i).enumerate() {
                        if let Some(&arg) = type_args.get(j) {
                            subst.insert(p.name, arg);
                        }
                    }
                    crate::solver::instantiate_type(self.ctx.types, constraint, &subst)
                } else {
                    constraint
                };

                let is_satisfied = {
                    let env = self.ctx.type_env.borrow();
                    let mut checker =
                        crate::solver::CompatChecker::with_resolver(self.ctx.types, &*env);
                    self.ctx.configure_compat_checker(&mut checker);
                    checker.is_assignable_to(type_arg, instantiated_constraint)
                };

                if !is_satisfied {
                    // Report TS2344 at the specific type argument node
                    if let Some(&arg_idx) = type_args_list.nodes.get(i) {
                        self.error_type_constraint_not_satisfied(
                            type_arg,
                            instantiated_constraint,
                            arg_idx,
                        );
                    }
                }
            }
        }
    }
}
