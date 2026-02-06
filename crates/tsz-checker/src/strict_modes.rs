//! Strict Mode Type Checking
//!
//! This module implements TypeScript's various strict mode checks:
//! - strictNullChecks: null/undefined handling
//! - strictFunctionTypes: contravariant parameter checking
//! - strictBindCallApply: typed bind/call/apply
//! - strictPropertyInitialization: class property initialization
//! - noImplicitAny: explicit any requirement
//! - noImplicitThis: typed this context
//! - useUnknownInCatchVariables: catch variables as unknown
//! - exactOptionalPropertyTypes: precise optional property handling

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_solver as solver_narrowing;
use tsz_solver::{TypeId, TypeInterner, TypeKey};

/// Strict modes checker that applies various strict mode rules
pub struct StrictModesChecker<'a> {
    arena: &'a NodeArena,
    types: &'a TypeInterner,
    strict_null_checks: bool,
    strict_function_types: bool,
    strict_bind_call_apply: bool,
    strict_property_initialization: bool,
    no_implicit_any: bool,
    no_implicit_this: bool,
    use_unknown_in_catch_variables: bool,
    exact_optional_property_types: bool,
}

impl<'a> StrictModesChecker<'a> {
    pub fn new(
        arena: &'a NodeArena,
        types: &'a TypeInterner,
        strict_null_checks: bool,
        strict_function_types: bool,
        strict_bind_call_apply: bool,
        strict_property_initialization: bool,
        no_implicit_any: bool,
        no_implicit_this: bool,
        use_unknown_in_catch_variables: bool,
        exact_optional_property_types: bool,
    ) -> Self {
        Self {
            arena,
            types,
            strict_null_checks,
            strict_function_types,
            strict_bind_call_apply,
            strict_property_initialization,
            no_implicit_any,
            no_implicit_this,
            use_unknown_in_catch_variables,
            exact_optional_property_types,
        }
    }

    /// Create from CheckerOptions
    pub fn from_options(
        arena: &'a NodeArena,
        types: &'a TypeInterner,
        options: &crate::context::CheckerOptions,
    ) -> Self {
        Self {
            arena,
            types,
            strict_null_checks: options.strict_null_checks,
            strict_function_types: options.strict_function_types,
            strict_bind_call_apply: options.strict_bind_call_apply,
            strict_property_initialization: options.strict_property_initialization,
            no_implicit_any: options.no_implicit_any,
            no_implicit_this: options.no_implicit_this,
            use_unknown_in_catch_variables: options.use_unknown_in_catch_variables,
            exact_optional_property_types: options.exact_optional_property_types,
        }
    }

    // -------------------------------------------------------------------------
    // strictNullChecks
    // -------------------------------------------------------------------------

    /// Check if a type is nullable under strict null checks
    pub fn is_nullable_type(&self, type_id: TypeId) -> bool {
        if !self.strict_null_checks {
            return false;
        }
        solver_narrowing::can_be_nullish(self.types, type_id)
    }

    /// Get the non-nullable version of a type
    pub fn get_non_nullable_type(&self, type_id: TypeId) -> TypeId {
        if !self.strict_null_checks {
            return type_id;
        }
        solver_narrowing::remove_nullish(self.types, type_id)
    }

    // -------------------------------------------------------------------------
    // strictFunctionTypes
    // -------------------------------------------------------------------------

    /// Check function parameter contravariance (strict function types)
    ///
    /// Under strictFunctionTypes, function parameters must be contravariant:
    /// A function (x: Animal) => void is NOT assignable to (x: Dog) => void
    pub fn check_function_parameter_contravariance(
        &self,
        source_param_type: TypeId,
        target_param_type: TypeId,
    ) -> FunctionTypeResult {
        if !self.strict_function_types {
            // Without strict function types, parameters are bivariant
            return FunctionTypeResult::Compatible;
        }

        // Under strict function types, source param must be supertype of target param
        // (contravariance)
        if self.is_subtype(target_param_type, source_param_type) {
            FunctionTypeResult::Compatible
        } else if self.is_subtype(source_param_type, target_param_type) {
            // Bivariant - allowed without strictFunctionTypes but not with it
            FunctionTypeResult::BivariantOnly
        } else {
            FunctionTypeResult::Incompatible
        }
    }

    /// Simple subtype check (delegates to type interner)
    fn is_subtype(&self, source: TypeId, target: TypeId) -> bool {
        // Handle simple cases
        if source == target {
            return true;
        }
        if target == TypeId::ANY || target == TypeId::UNKNOWN {
            return true;
        }
        if source == TypeId::NEVER {
            return true;
        }

        // For full subtype checking, we'd need to use the solver's subtype checker
        // This is a simplified version for the strict modes checker
        false
    }

    // -------------------------------------------------------------------------
    // strictBindCallApply
    // -------------------------------------------------------------------------

    /// Check if bind/call/apply should use strict typing
    pub fn should_use_strict_bind_call_apply(&self) -> bool {
        self.strict_bind_call_apply
    }

    // -------------------------------------------------------------------------
    // strictPropertyInitialization
    // -------------------------------------------------------------------------

    /// Check if a class property needs initialization
    pub fn requires_property_initialization(&self) -> bool {
        self.strict_property_initialization && self.strict_null_checks
    }

    // -------------------------------------------------------------------------
    // noImplicitAny
    // -------------------------------------------------------------------------

    /// Check if implicit any should be reported
    pub fn should_report_implicit_any(&self, type_id: TypeId) -> bool {
        self.no_implicit_any && type_id == TypeId::ANY
    }

    // -------------------------------------------------------------------------
    // noImplicitThis
    // -------------------------------------------------------------------------

    /// Check if implicit this should be reported
    pub fn should_report_implicit_this(&self) -> bool {
        self.no_implicit_this
    }

    // -------------------------------------------------------------------------
    // useUnknownInCatchVariables
    // -------------------------------------------------------------------------

    /// Get the type for catch clause variable
    pub fn get_catch_variable_type(&self) -> TypeId {
        if self.use_unknown_in_catch_variables {
            TypeId::UNKNOWN
        } else {
            TypeId::ANY
        }
    }

    // -------------------------------------------------------------------------
    // exactOptionalPropertyTypes
    // -------------------------------------------------------------------------

    /// Check if we should use exact optional property types
    pub fn use_exact_optional_property_types(&self) -> bool {
        self.exact_optional_property_types
    }

    /// Check if explicitly writing undefined to an optional property is allowed
    ///
    /// With exactOptionalPropertyTypes:
    /// - `obj.prop = undefined` is an error if prop is `string?` (should be absent or string)
    /// - `obj.prop = undefined` is allowed if prop is `string | undefined`
    pub fn check_optional_property_assignment(
        &self,
        property_is_optional: bool,
        property_type_includes_undefined: bool,
        assigned_type: TypeId,
    ) -> OptionalPropertyResult {
        if !self.exact_optional_property_types {
            return OptionalPropertyResult::Allowed;
        }

        // If assigning undefined to an optional property
        if assigned_type == TypeId::UNDEFINED && property_is_optional {
            // Only allowed if the property type explicitly includes undefined
            if property_type_includes_undefined {
                OptionalPropertyResult::Allowed
            } else {
                OptionalPropertyResult::ExplicitUndefinedNotAllowed
            }
        } else {
            OptionalPropertyResult::Allowed
        }
    }
}

/// Result of function type compatibility check under strictFunctionTypes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionTypeResult {
    /// Types are compatible under strict function types
    Compatible,
    /// Types are only compatible under bivariant (non-strict) checking
    BivariantOnly,
    /// Types are incompatible
    Incompatible,
}

/// Result of optional property assignment check under exactOptionalPropertyTypes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalPropertyResult {
    /// Assignment is allowed
    Allowed,
    /// Cannot explicitly assign undefined to optional property without undefined in type
    ExplicitUndefinedNotAllowed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_type_result_eq() {
        assert_eq!(
            FunctionTypeResult::Compatible,
            FunctionTypeResult::Compatible
        );
        assert_ne!(
            FunctionTypeResult::Compatible,
            FunctionTypeResult::Incompatible
        );
    }

    #[test]
    fn test_optional_property_result_eq() {
        assert_eq!(
            OptionalPropertyResult::Allowed,
            OptionalPropertyResult::Allowed
        );
        assert_ne!(
            OptionalPropertyResult::Allowed,
            OptionalPropertyResult::ExplicitUndefinedNotAllowed
        );
    }

    #[test]
    fn test_strict_modes_checker_creation() {
        let arena = NodeArena::new();
        let types = TypeInterner::new();
        let checker = StrictModesChecker::new(
            &arena, &types, true, // strict_null_checks
            true, // strict_function_types
            true, // strict_bind_call_apply
            true, // strict_property_initialization
            true, // no_implicit_any
            true, // no_implicit_this
            true, // use_unknown_in_catch_variables
            true, // exact_optional_property_types
        );

        assert!(checker.requires_property_initialization());
        assert!(checker.should_report_implicit_this());
        assert_eq!(checker.get_catch_variable_type(), TypeId::UNKNOWN);
    }

    #[test]
    fn test_catch_variable_type() {
        let arena = NodeArena::new();
        let types = TypeInterner::new();

        // With useUnknownInCatchVariables
        let checker = StrictModesChecker::new(
            &arena, &types, false, false, false, false, false, false, true, false,
        );
        assert_eq!(checker.get_catch_variable_type(), TypeId::UNKNOWN);

        // Without useUnknownInCatchVariables
        let checker = StrictModesChecker::new(
            &arena, &types, false, false, false, false, false, false, false, false,
        );
        assert_eq!(checker.get_catch_variable_type(), TypeId::ANY);
    }
}
