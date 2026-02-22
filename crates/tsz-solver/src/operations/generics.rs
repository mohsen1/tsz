//! Generic type instantiation validation.
//!
//! Validates explicit type arguments against their type parameter constraints.
//! Used by the checker when explicit type arguments are provided to a generic
//! (e.g., `foo<number>(x)`).

use crate::TypeDatabase;
use crate::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::AssignabilityChecker;
use crate::types::{TypeId, TypeParamInfo};
use tsz_common::interner::Atom;

/// Result of validating type arguments against their type parameter constraints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenericInstantiationResult {
    /// All type arguments satisfy their constraints
    Success,
    /// A type argument doesn't satisfy its type parameter constraint
    ConstraintViolation {
        /// Index of the type parameter that failed
        param_index: usize,
        /// Name of the type parameter that failed
        param_name: Atom,
        /// The constraint type
        constraint: TypeId,
        /// The provided type argument that doesn't satisfy the constraint
        type_arg: TypeId,
    },
}

/// Validate type arguments against their type parameter constraints.
///
/// This function is used when explicit type arguments are provided to a generic.
/// It ensures that each type argument satisfies its corresponding type parameter's
/// constraint, emitting errors instead of silently falling back to `Any`.
///
/// # Arguments
/// * `type_params` - The declared type parameters (e.g., `<T extends string, U>`)
/// * `type_args` - The provided type arguments (e.g., `<number, boolean>`)
/// * `checker` - The assignability checker to use for constraint validation
///
/// # Returns
/// * `GenericInstantiationResult::Success` if all constraints are satisfied
/// * `GenericInstantiationResult::ConstraintViolation` if any constraint is violated
pub fn solve_generic_instantiation<C: AssignabilityChecker>(
    type_params: &[TypeParamInfo],
    type_args: &[TypeId],
    interner: &dyn TypeDatabase,
    checker: &mut C,
) -> GenericInstantiationResult {
    for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
        if let Some(constraint) = param.constraint {
            // Constraints may reference earlier type parameters, so instantiate them
            let instantiated_constraint = if i > 0 {
                let mut subst = TypeSubstitution::new();
                for (j, p) in type_params.iter().take(i).enumerate() {
                    if let Some(&arg) = type_args.get(j) {
                        subst.insert(p.name, arg);
                    }
                }
                instantiate_type(interner, constraint, &subst)
            } else {
                constraint
            };

            // Validate that the type argument satisfies the constraint
            if !checker.is_assignable_to(type_arg, instantiated_constraint) {
                return GenericInstantiationResult::ConstraintViolation {
                    param_index: i,
                    param_name: param.name,
                    constraint: instantiated_constraint,
                    type_arg,
                };
            }
        }
    }
    GenericInstantiationResult::Success
}
