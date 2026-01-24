//! Constructor Type Checking Utilities Module
//!
//! This module contains constructor type checking utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Constructor accessibility checking (private, protected, public)
//! - Constructor signature utilities
//! - Constructor instantiation validation
//!
//! This module extends CheckerState with utilities for constructor-related
//! type checking operations.

use crate::checker::state::CheckerState;
use crate::solver::TypeId;

// =============================================================================
// Constructor Type Checking Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Constructor Accessibility
    // =========================================================================

    /// Check if a type is an abstract constructor type.
    ///
    /// Abstract constructors cannot be instantiated directly with `new`.
    pub fn is_abstract_ctor(&self, type_id: TypeId) -> bool {
        self.ctx.abstract_constructor_types.contains(&type_id)
    }

    /// Check if a type is a private constructor.
    ///
    /// Private constructors can only be called from within the class.
    pub fn is_private_ctor(&self, type_id: TypeId) -> bool {
        self.ctx.private_constructor_types.contains(&type_id)
    }

    /// Check if a type is a protected constructor.
    ///
    /// Protected constructors can be called from the class and its subclasses.
    pub fn is_protected_ctor(&self, type_id: TypeId) -> bool {
        self.ctx.protected_constructor_types.contains(&type_id)
    }

    /// Check if a type is a public constructor.
    ///
    /// Public constructors have no access restrictions.
    pub fn is_public_ctor(&self, type_id: TypeId) -> bool {
        !self.is_private_ctor(type_id) && !self.is_protected_ctor(type_id)
    }

    // =========================================================================
    // Constructor Signature Utilities
    // =========================================================================

    /// Check if a type has any construct signature.
    ///
    /// Construct signatures allow a type to be called with `new`.
    pub fn has_construct_sig(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                !shape.construct_signatures.is_empty()
            }
            _ => false,
        }
    }

    /// Get the number of construct signatures for a type.
    ///
    /// Multiple construct signatures indicate constructor overloading.
    pub fn construct_signature_count(&self, type_id: TypeId) -> usize {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                shape.construct_signatures.len()
            }
            _ => 0,
        }
    }

    // =========================================================================
    // Constructor Instantiation
    // =========================================================================

    /// Check if a constructor can be instantiated.
    ///
    /// Returns false for abstract constructors which cannot be instantiated.
    pub fn can_instantiate(&self, constructor_type: TypeId) -> bool {
        !self.is_abstract_ctor(constructor_type)
    }

    /// Check if `new` can be applied to a type.
    ///
    /// This is a convenience check combining constructor type detection
    /// with abstract constructor checking.
    pub fn can_use_new(&self, type_id: TypeId) -> bool {
        self.has_construct_sig(type_id) && self.can_instantiate(type_id)
    }

    /// Check if a type is a class constructor (typeof Class).
    ///
    /// Returns true for Callable types with only construct signatures (no call signatures).
    /// This is used to detect when a class constructor is being called without `new`.
    pub fn is_class_constructor_type(&self, type_id: TypeId) -> bool {
        // A class constructor is a Callable with construct signatures but no call signatures
        self.has_construct_sig(type_id) && !self.has_call_signature(type_id)
    }

    /// Check if two constructor types have compatible accessibility.
    ///
    /// Returns true if source can be assigned to target based on their constructor accessibility.
    /// - Public constructors are compatible with everything
    /// - Private constructors are only compatible with the same private constructor
    /// - Protected constructors are compatible with protected or public targets
    pub fn ctor_access_compatible(&self, source: TypeId, target: TypeId) -> bool {
        // Public constructors are compatible with everything
        if !self.is_private_ctor(source) && !self.is_protected_ctor(source) {
            return true;
        }

        // Private constructors are only compatible with the same private constructor
        if self.is_private_ctor(source) {
            if self.is_private_ctor(target) {
                source == target
            } else {
                false
            }
        } else {
            // Protected constructors are compatible with protected or public targets
            !self.is_private_ctor(target)
        }
    }

    /// Check if a type should be treated as a constructor in `new` expressions.
    ///
    /// This determines if a type can be used with the `new` operator.
    pub fn is_newable(&self, type_id: TypeId) -> bool {
        self.has_construct_sig(type_id)
    }
}
