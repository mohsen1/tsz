//! Exact declaration targets for type-only import applications.

use crate::state::CheckerState;
use crate::state_domain::type_analysis::source_file_import_binding::source_file_import_binding_symbol;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeAccess;

pub(crate) struct TypeOnlyImportApplicationTarget {
    pub(crate) sym_id: SymbolId,
    pub(crate) def_id: tsz_solver::def::DefId,
}

impl CheckerState<'_> {
    /// Return the current binder's exact default type-import alias when lexical
    /// resolution selects it without a nearer type parameter, declaration,
    /// namespace, module, merge partner, or module-augmentation context.
    pub(crate) fn lexically_selected_explicit_default_type_import_alias(
        &self,
        type_name_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<SymbolId> {
        let name = self.ctx.arena.get_identifier_text(type_name_idx)?;
        let alias_sym_id = source_file_import_binding_symbol(self.ctx.arena, self.ctx.binder, name)
            .or_else(|| self.ctx.binder.file_locals.get(name))?;
        let alias_symbol = self.ctx.binder.get_symbol(alias_sym_id)?;
        if !alias_symbol.has_any_flags(symbol_flags::ALIAS)
            || !alias_symbol.is_type_only
            || alias_symbol.import_module().is_none()
            || alias_symbol.import_name() != Some("default")
            || alias_symbol.import_resolution_mode().is_some()
            || alias_symbol.declarations.len() != 1
            || alias_symbol.is_umd_export
            || !self.reference_symbol_is_import_alias(alias_symbol)
            || self
                .resolve_enclosing_type_parameter_symbol(type_name_idx, name)
                .is_some()
            || self.is_inside_module_augmentation(type_name_idx)
        {
            return None;
        }

        let selected = self.ctx.binder.resolve_identifier_with_filter(
            self.ctx.arena,
            type_name_idx,
            &[],
            |candidate| {
                self.ctx.binder.get_symbol(candidate).is_some_and(|symbol| {
                    symbol.has_any_flags(
                        symbol_flags::TYPE
                            | symbol_flags::ALIAS
                            | symbol_flags::NAMESPACE_MODULE
                            | symbol_flags::VALUE_MODULE,
                    )
                })
            },
        );
        if selected != Some(alias_sym_id)
            || self
                .ctx
                .alias_partner_for(self.ctx.binder, alias_sym_id)
                .is_some()
            || self
                .ctx
                .alias_partner_reverse(self.ctx.binder, alias_sym_id)
                .is_some()
        {
            return None;
        }
        Some(alias_sym_id)
    }

    /// Resolve an explicit default import of a pure generic type alias to its
    /// exact declaration identity.
    ///
    /// This owner-qualified route must run before general type-position alias
    /// resolution. That legacy path publishes binder-local raw symbol owners
    /// while probing export tables, which can overwrite an unrelated local
    /// symbol with the same `SymbolId`.
    pub(crate) fn resolve_explicit_default_type_alias_application_target(
        &mut self,
        alias_sym_id: SymbolId,
    ) -> Option<TypeOnlyImportApplicationTarget> {
        let alias_symbol = self.ctx.binder.get_symbol(alias_sym_id)?;
        if !alias_symbol.has_any_flags(symbol_flags::ALIAS) || !alias_symbol.is_type_only {
            return None;
        }

        let module_name = alias_symbol.import_module().map(str::to_string)?;
        if alias_symbol.import_name() != Some("default") {
            return None;
        }
        let (target_sym_id, target_file_idx) = self
            .explicit_default_export_pure_type_alias_identity(
                &module_name,
                self.ctx.current_file_idx,
                true,
            )?;
        self.try_resolve_cross_arena_named_alias_without_child(alias_sym_id)?;
        let target_symbol = self
            .ctx
            .get_binder_for_file(target_file_idx)?
            .get_symbol(target_sym_id)?;
        let def_id = self.ctx.def_id_for_declaration_in_file(
            target_sym_id,
            target_file_idx,
            &target_symbol.escaped_name,
        )?;
        Some(TypeOnlyImportApplicationTarget {
            sym_id: target_sym_id,
            def_id,
        })
    }

    pub(crate) fn resolve_type_only_import_alias_target_symbol(
        &mut self,
        name: &str,
    ) -> Option<SymbolId> {
        let alias_sym_id = source_file_import_binding_symbol(self.ctx.arena, self.ctx.binder, name)
            .or_else(|| self.ctx.binder.file_locals.get(name))?;
        let alias_symbol = self.ctx.binder.get_symbol(alias_sym_id)?;
        if !alias_symbol.has_any_flags(symbol_flags::ALIAS) || !alias_symbol.is_type_only {
            return None;
        }
        let module_name = alias_symbol.import_module().map(str::to_string)?;
        let import_name = alias_symbol.import_name().unwrap_or(name).to_owned();
        let target_sym_id = self.resolve_cross_file_export_from_file(
            &module_name,
            &import_name,
            Some(self.ctx.current_file_idx),
        )?;
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index_stable(target_sym_id) {
            self.ctx
                .register_symbol_file_target(target_sym_id, file_idx);
        }
        Some(target_sym_id)
    }

    /// Resolve a type-only import used as a generic application to the exact
    /// declaration `DefId` that owns its body and parameters.
    ///
    /// Explicit default exports of pure generic aliases bypass the binder's
    /// synthetic `default` symbol. The direct alias shortcut proves and
    /// publishes the actual declaration first, then this method returns its
    /// file-qualified `DefId` without persisting a raw-`SymbolId` owner override.
    pub(crate) fn resolve_type_only_import_application_target(
        &mut self,
        name: &str,
    ) -> Option<TypeOnlyImportApplicationTarget> {
        let alias_sym_id = source_file_import_binding_symbol(self.ctx.arena, self.ctx.binder, name)
            .or_else(|| self.ctx.binder.file_locals.get(name))?;
        if let Some(target) =
            self.resolve_explicit_default_type_alias_application_target(alias_sym_id)
        {
            return Some(target);
        }

        let alias_symbol = self.ctx.binder.get_symbol(alias_sym_id)?;
        if !alias_symbol.has_any_flags(symbol_flags::ALIAS) || !alias_symbol.is_type_only {
            return None;
        }

        alias_symbol.import_module()?;
        let target_sym_id = self.resolve_type_only_import_alias_target_symbol(name)?;
        let target_name = self
            .get_cross_file_symbol(target_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(target_sym_id))
            .map(|symbol| symbol.escaped_name.clone())
            .unwrap_or_else(|| name.to_string());
        self.ensure_def_ready_for_lowering(target_sym_id, &target_name);
        let def_id = self
            .reexported_declaration_def_id_for_lowering(alias_sym_id, name)
            .unwrap_or_else(|| {
                self.ctx
                    .get_or_create_def_id_for_symbol_name(target_sym_id, &target_name)
            });
        Some(TypeOnlyImportApplicationTarget {
            sym_id: target_sym_id,
            def_id,
        })
    }
}
