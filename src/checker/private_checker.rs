//! Private Field Checking Module
//!
//! This module contains methods for validating private field access and brands.
//! It handles:
//! - Private brand extraction and comparison
//! - Private field name extraction
//! - Brand mismatch error generation
//!
//! Private members in TypeScript classes use a "brand" property for nominal typing.
//! This ensures private members can only be accessed from instances of the same class.
//!
//! This module extends CheckerState with private field methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::checker::state::CheckerState;
use crate::solver::TypeId;

// =============================================================================
// Private Field Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Private Brand Extraction
    // =========================================================================

    /// Get the private brand property from a type.
    ///
    /// Private members in classes use a "brand" property for nominal typing.
    /// This brand is a property named like `__private_brand_#className`.
    ///
    /// Returns Some(brand_name) if the type has a private brand.
    pub(crate) fn get_private_brand(&self, type_id: TypeId) -> Option<String> {
        use crate::solver::type_queries::{PrivateBrandKind, classify_for_private_brand};

        match classify_for_private_brand(self.ctx.types, type_id) {
            PrivateBrandKind::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in &shape.properties {
                    let name = self.ctx.types.resolve_atom(prop.name);
                    if name.starts_with("__private_brand_") {
                        return Some(name);
                    }
                }
                None
            }
            PrivateBrandKind::Callable(callable_id) => {
                let callable = self.ctx.types.callable_shape(callable_id);
                for prop in &callable.properties {
                    let name = self.ctx.types.resolve_atom(prop.name);
                    if name.starts_with("__private_brand_") {
                        return Some(name);
                    }
                }
                None
            }
            PrivateBrandKind::None => None,
        }
    }

    // =========================================================================
    // Private Brand Comparison
    // =========================================================================

    /// Check if two types have the same private brand.
    ///
    /// This is used for nominal typing of private member access. Private members
    /// can only be accessed from instances of the same class that declared them.
    ///
    /// Returns true if both types have the same private brand.
    pub(crate) fn types_have_same_private_brand(&self, type1: TypeId, type2: TypeId) -> bool {
        match (self.get_private_brand(type1), self.get_private_brand(type2)) {
            (Some(brand1), Some(brand2)) => brand1 == brand2,
            _ => false,
        }
    }

    // =========================================================================
    // Private Field Name Extraction
    // =========================================================================

    /// Extract the name of the private field from a brand string.
    ///
    /// Given a type with a private brand, returns the actual private field name
    /// (e.g., "#foo") if found.
    ///
    /// Returns Some(private_field_name) if found, None otherwise.
    pub(crate) fn get_private_field_name_from_brand(&self, type_id: TypeId) -> Option<String> {
        use crate::solver::type_queries::{PrivateBrandKind, classify_for_private_brand};

        let properties = match classify_for_private_brand(self.ctx.types, type_id) {
            PrivateBrandKind::Object(shape_id) => {
                self.ctx.types.object_shape(shape_id).properties.clone()
            }
            PrivateBrandKind::Callable(callable_id) => self
                .ctx
                .types
                .callable_shape(callable_id)
                .properties
                .clone(),
            PrivateBrandKind::None => return None,
        };

        // Find the first non-brand private property (starts with #)
        for prop in &properties {
            let name = self.ctx.types.resolve_atom(prop.name);
            if name.starts_with('#') && !name.starts_with("__private_brand_") {
                return Some(name);
            }
        }
        None
    }

    // =========================================================================
    // Private Brand Mismatch Error
    // =========================================================================

    /// Check if there's a private brand mismatch between two types.
    ///
    /// When accessing a private member, TypeScript checks that the object has the same
    /// private brand as the class declaring the member. This function generates an
    /// appropriate error message for mismatches.
    ///
    /// Returns Some(error_message) if there's a private brand mismatch.
    pub(crate) fn private_brand_mismatch_error(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let source_brand = self.get_private_brand(source)?;
        let target_brand = self.get_private_brand(target)?;

        if source_brand == target_brand {
            return None;
        }

        let field_name = self
            .get_private_field_name_from_brand(source)
            .unwrap_or_else(|| "[private field]".to_string());

        Some(format!(
            "Property '{}' in type '{}' refers to a different member that cannot be accessed from within type '{}'.",
            field_name,
            self.format_type(source),
            self.format_type(target)
        ))
    }
}
