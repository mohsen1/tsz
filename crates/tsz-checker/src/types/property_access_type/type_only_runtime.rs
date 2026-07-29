//! Type-only import classification for property-access value contexts.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

pub(super) struct TypeOnlyRuntimeBase {
    pub(super) type_only_alias_without_local_value: bool,
    pub(super) has_exact_augmentation_runtime: bool,
}

impl<'a> CheckerState<'a> {
    pub(super) fn classify_type_only_property_access_base(
        &self,
        expression: NodeIndex,
    ) -> TypeOnlyRuntimeBase {
        let type_only_alias_without_local_value = self
            .ctx
            .arena
            .get_identifier_at(expression)
            .is_some_and(|base_identifier| {
                let base_identifier_name = base_identifier.escaped_text.as_str();
                self.resolve_identifier_symbol(expression)
                    .or_else(|| {
                        self.ctx
                            .binder
                            .resolve_identifier(self.ctx.arena, expression)
                    })
                    .is_some_and(|base_sym_id| self.alias_resolves_to_type_only(base_sym_id))
                    && !self.source_file_has_value_import_binding_named(
                        expression,
                        base_identifier_name,
                    )
                    && self
                        .local_current_file_value_symbol_named(base_identifier_name)
                        .is_none()
            });
        let has_exact_augmentation_runtime = type_only_alias_without_local_value
            && self
                .ctx
                .arena
                .get_identifier_at(expression)
                .is_some_and(|base_identifier| {
                    self.named_import_augmentation_runtime_provenance(
                        expression,
                        &base_identifier.escaped_text,
                    )
                    .is_some()
                });

        TypeOnlyRuntimeBase {
            type_only_alias_without_local_value,
            has_exact_augmentation_runtime,
        }
    }
}
