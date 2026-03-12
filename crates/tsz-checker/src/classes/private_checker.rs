//! Private field access and brand checking for nominal class typing.

use crate::diagnostics::{diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_solver::TypeId;

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
    /// Returns `Some(brand_name)` if the type has a private brand.
    pub(crate) fn get_private_brand(&self, type_id: TypeId) -> Option<String> {
        tsz_solver::type_queries::get_private_brand_name(self.ctx.types, type_id)
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
    /// Returns `Some(private_field_name)` if found, None otherwise.
    pub(crate) fn get_private_field_name_from_brand(&self, type_id: TypeId) -> Option<String> {
        tsz_solver::type_queries::get_private_field_name(self.ctx.types, type_id)
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
    /// Returns `Some(error_message)` if there's a private brand mismatch.
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

        let shared_nominal_member = |type_id: TypeId| {
            tsz_solver::type_queries::get_object_shape(self.ctx.types, type_id).and_then(|shape| {
                shape.properties.iter().find_map(|prop| {
                    let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                    (!prop_name.starts_with("__private_brand_")
                        && prop.visibility != tsz_solver::Visibility::Public)
                        .then(|| (prop_name.to_string(), prop.visibility))
                })
            })
        };

        if let (Some((source_member, source_visibility)), Some((target_member, target_visibility))) =
            (shared_nominal_member(source), shared_nominal_member(target))
            && source_member == target_member
            && source_visibility == target_visibility
        {
            return Some(match source_visibility {
                tsz_solver::Visibility::Private => format_message(
                    diagnostic_messages::TYPES_HAVE_SEPARATE_DECLARATIONS_OF_A_PRIVATE_PROPERTY,
                    &[&source_member],
                ),
                tsz_solver::Visibility::Protected => {
                    format!(
                        "Types have separate declarations of a protected property '{source_member}'."
                    )
                }
                tsz_solver::Visibility::Public => {
                    unreachable!("public members do not create nominal brands")
                }
            });
        }

        let field_name = shared_nominal_member(source)
            .map(|(member_name, _)| member_name)
            .or_else(|| self.get_private_field_name_from_brand(source))
            .unwrap_or_else(|| "[private field]".to_string());

        Some(format!(
            "Property '{}' in type '{}' refers to a different member that cannot be accessed from within type '{}'.",
            field_name,
            self.format_type(source),
            self.format_type(target)
        ))
    }
}
