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
        crate::query_boundaries::common::get_private_brand_name(self.ctx.types, type_id)
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
        crate::query_boundaries::common::get_private_field_name(self.ctx.types, type_id)
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
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
                .and_then(|shape| {
                    shape.properties.iter().find_map(|prop| {
                        let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                        (!tsz_solver::utils::is_synthetic_private_brand_name(&prop_name)
                            && prop.visibility != tsz_solver::Visibility::Public)
                            .then(|| (prop_name.to_string(), prop.visibility, prop.parent_id))
                    })
                })
        };

        if let (
            Some((source_member, source_visibility, source_parent)),
            Some((target_member, target_visibility, target_parent)),
        ) = (shared_nominal_member(source), shared_nominal_member(target))
            && source_member == target_member
            && source_visibility == target_visibility
        {
            // An ES private identifier (`#name`) is a per-class slot: tsc
            // reports TS18015 ("refers to a different member") for it, and
            // reserves the "separate declarations" wording for
            // modifier-`private` members.
            if tsz_solver::utils::is_es_private_identifier_name(&source_member) {
                return Some(format_message(
                    diagnostic_messages::PROPERTY_IN_TYPE_REFERS_TO_A_DIFFERENT_MEMBER_THAT_CANNOT_BE_ACCESSED_FROM_WITHI,
                    &[&source_member, &self.format_type(source), &self.format_type(target)],
                ));
            }
            return match source_visibility {
                tsz_solver::Visibility::Private => Some(format_message(
                    diagnostic_messages::TYPES_HAVE_SEPARATE_DECLARATIONS_OF_A_PRIVATE_PROPERTY,
                    &[&source_member],
                )),
                tsz_solver::Visibility::Protected => self.protected_brand_mismatch_error(
                    &source_member,
                    source,
                    target,
                    source_parent,
                    target_parent,
                ),
                tsz_solver::Visibility::Public => {
                    unreachable!("public members do not create nominal brands")
                }
            };
        }

        let field_name = shared_nominal_member(source)
            .map(|(member_name, _, _)| member_name)
            .or_else(|| self.get_private_field_name_from_brand(source))
            .unwrap_or_else(|| "[private field]".to_string());

        Some(format_message(
            diagnostic_messages::PROPERTY_IN_TYPE_REFERS_TO_A_DIFFERENT_MEMBER_THAT_CANNOT_BE_ACCESSED_FROM_WITHI,
            &[
                &field_name,
                &self.format_type(source),
                &self.format_type(target),
            ],
        ))
    }

    // =========================================================================
    // Protected Brand Mismatch Error
    // =========================================================================

    /// tsc's `propertyRelatedTo` never treats two differently-declared
    /// `protected` members as a "separate declarations" mismatch the way it
    /// does for `private`. It instead runs `isValidOverrideOf`: the mismatch
    /// is only an error when the source's declaring class is *not* derived
    /// from the target's declaring class (`hasBaseType`); a subclass legally
    /// narrowing an inherited protected member is not an error at all. When
    /// it does fail, tsc reports `TS2443` ("Property '{0}' is protected but
    /// type '{1}' is not a class derived from '{2}'.") naming each side's
    /// declaring class, not the receiver type itself.
    ///
    /// Returns `None` when the override is valid (no mismatch to report).
    pub(crate) fn protected_brand_mismatch_error(
        &self,
        member_name: &str,
        source: TypeId,
        target: TypeId,
        source_parent: Option<tsz_binder::SymbolId>,
        target_parent: Option<tsz_binder::SymbolId>,
    ) -> Option<String> {
        if let (Some(source_sym), Some(target_sym)) = (source_parent, target_parent)
            && (source_sym == target_sym
                || self
                    .ctx
                    .inheritance_graph
                    .is_derived_from(source_sym, target_sym))
        {
            return None;
        }

        let declaring_class_display_name = |sym: Option<tsz_binder::SymbolId>, fallback: TypeId| {
            sym.and_then(|sym| self.get_class_declaration_from_symbol(sym))
                .map(|class_idx| self.get_syntactic_class_name_or_anonymous(class_idx))
                .unwrap_or_else(|| self.format_type(fallback))
        };

        Some(format_message(
            diagnostic_messages::PROPERTY_IS_PROTECTED_BUT_TYPE_IS_NOT_A_CLASS_DERIVED_FROM,
            &[
                member_name,
                &declaring_class_display_name(source_parent, source),
                &declaring_class_display_name(target_parent, target),
            ],
        ))
    }
}
