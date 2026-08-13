//! Failure explanation for object index-signature property checks.

use crate::def::resolver::TypeResolver;
use crate::diagnostics::SubtypeFailureReason;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{ObjectShape, PropertyInfo};
use crate::utils;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(in crate::relations::subtype) fn explain_properties_against_index_signatures(
        &mut self,
        source: &[PropertyInfo],
        target: &ObjectShape,
    ) -> Option<SubtypeFailureReason> {
        let string_index = target.string_index_signature();
        let symbol_index = target.symbol_index_signature();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() && symbol_index.is_none() {
            return None;
        }

        for prop in source {
            // Strip `undefined` from optional property types when checking against
            // index signatures, matching tsc behavior.
            let prop_type = if prop.optional {
                crate::narrowing::utils::remove_undefined(self.interner, prop.type_id)
            } else {
                prop.type_id
            };
            let allow_bivariant = false;

            if let Some(number_idx) = number_index {
                let is_numeric = utils::is_numeric_property_name(self.interner, prop.name);
                if is_numeric {
                    if !number_idx.readonly && prop.readonly {
                        return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                            property_name: prop.name,
                        });
                    }
                    if !self
                        .check_subtype_with_method_variance(
                            prop_type,
                            number_idx.value_type,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        return self.make_index_sig_reason(
                            "number",
                            prop_type,
                            number_idx.value_type,
                            Some(prop.name),
                        );
                    }
                }
            }

            if let Some(string_idx) = string_index
                && !prop.is_symbol_named
            {
                if !string_idx.readonly && prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: prop.name,
                    });
                }
                if !self
                    .check_subtype_with_method_variance(
                        prop_type,
                        string_idx.value_type,
                        allow_bivariant,
                    )
                    .is_true()
                {
                    return self.make_index_sig_reason(
                        "string",
                        prop_type,
                        string_idx.value_type,
                        Some(prop.name),
                    );
                }
            }

            if let Some(symbol_idx) = symbol_index
                && prop.is_symbol_named
                && !self
                    .check_subtype_with_method_variance(
                        prop_type,
                        symbol_idx.value_type,
                        allow_bivariant,
                    )
                    .is_true()
            {
                return self.make_index_sig_reason(
                    "symbol",
                    prop_type,
                    symbol_idx.value_type,
                    Some(prop.name),
                );
            }
        }

        None
    }
}
