//! Symbol index-signature subtype rules.

use crate::types::{ObjectShape, TypeId};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check symbol index signature compatibility between source and target.
    ///
    /// A source wide-symbol index must cover the target key space and its value
    /// type must be assignable to the target value type. Anonymous/inferable
    /// sources without a symbol index fall through to the structural
    /// symbol-property checks; named class/interface sources must declare an
    /// explicit symbol index, except for an optional target index and an empty
    /// source shape.
    pub(crate) fn check_symbol_index_compatibility(
        &mut self,
        source: &ObjectShape,
        source_receiver: Option<TypeId>,
        target: &ObjectShape,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        let Some(t_symbol_idx) = target.symbol_index_signature() else {
            return SubtypeResult::True;
        };

        match source.symbol_index_signature() {
            Some(s_symbol_idx)
                if self
                    .index_signature_key_covers(s_symbol_idx.key_type, t_symbol_idx.key_type) =>
            {
                let source_value =
                    self.bind_property_receiver_this(source_receiver, s_symbol_idx.value_type);
                let target_value =
                    self.bind_property_receiver_this(target_receiver, t_symbol_idx.value_type);
                if self.check_subtype(source_value, target_value).is_true() {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                }
            }
            _ => {
                if target.symbol_index_is_optional() && source.properties.is_empty() {
                    return SubtypeResult::True;
                }
                if self.requires_explicit_declared_index_signature_for(source, source_receiver) {
                    return SubtypeResult::False;
                }
                SubtypeResult::True
            }
        }
    }
}
