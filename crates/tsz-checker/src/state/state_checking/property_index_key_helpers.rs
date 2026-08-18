//! String index key helpers for excess-property diagnostics.
//!
//! Extracted from `property.rs` to keep the oversized state-checking file moving
//! toward the repository line-count limit while routing diagnostic relation
//! checks through the shared boundary.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn string_index_key_accepts_property_name(
        &mut self,
        key_type: TypeId,
        prop_name: &str,
        is_symbol_named: bool,
    ) -> bool {
        if key_type == TypeId::SYMBOL {
            return is_symbol_named
                || prop_name.starts_with("[Symbol.")
                || prop_name.starts_with("__unique_")
                || prop_name.starts_with("__@");
        }

        // A symbol-named property is never covered by a string-keyed index
        // signature (tsc's `getApplicableIndexInfo` applies only a symbol
        // index info to it), so the broad-`string` acceptance below must not
        // claim it (#17623).
        if is_symbol_named {
            return false;
        }

        if key_type == TypeId::STRING {
            return true;
        }

        let prop_literal =
            crate::query_boundaries::common::create_string_literal_type(self.ctx.types, prop_name);
        self.property_index_key_relation_outcome(prop_literal, key_type)
            .related
    }

    pub(super) fn index_value_type_is_deferred(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::is_index_access_type(self.ctx.types, type_id)
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, type_id)
    }
}
