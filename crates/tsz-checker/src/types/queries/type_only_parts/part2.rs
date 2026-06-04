impl<'a> CheckerState<'a> {
    /// Resolve a module specifier from a given source file, then check if
    /// `export_name` in that target module is type-only.
    fn is_export_type_only_in_file(
        &self,
        source_file_idx: usize,
        module_specifier: &str,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<(usize, String)>,
    ) -> bool {
        const PURE_TYPE: u32 =
            symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS | symbol_flags::TYPE_PARAMETER;

        // Resolve the specifier to a target file index
        let Some(target_file_idx) = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)
        else {
            return false;
        };

        let key = (target_file_idx, export_name.to_string());
        if !visited.insert(key) {
            return false; // cycle
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };

        // Get the target file's canonical name (module_exports key)
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };

        // Check direct exports in target binder
        if let Some(exports_table) = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            && let Some(sym_id) = exports_table.get(export_name)
        {
            // Look up the symbol using the target binder first (which owns the export),
            // then fall back to the main binder (for merged/remapped symbol arenas in
            // the full pipeline). In per-file binder setups, SymbolIds are local to each
            // file, so `self.ctx.binder.get_symbol(sym_id)` may return a wrong symbol
            // from the current file at the same index.
            let sym_opt = target_binder
                .get_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id));
            if let Some(sym) = sym_opt {
                const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
                const VALUE: u32 = symbol_flags::VARIABLE
                    | symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::ENUM
                    | symbol_flags::ENUM_MEMBER
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE;

                if sym.is_type_only {
                    // A merged symbol like `import type { A }` + `const A = 0`
                    // has both ALIAS and VALUE flags. The value binding overrides
                    // type-only status. But cloned `export type { A as default }`
                    // symbols copy the source's value flags (e.g., CLASS) without
                    // ALIAS, so we only skip when ALIAS+VALUE are both present.
                    let has_value_flags = sym.has_any_flags(symbol_flags::ALIAS)
                        && sym.has_any_flags(symbol_flags::VALUE);
                    let has_value_partner =
                        self.ctx.alias_partners_contains(self.ctx.binder, sym_id);
                    if !has_value_flags && !has_value_partner {
                        return true;
                    }
                }
                // A pure type-alias/interface symbol can be paired with an
                // `export * as Name from "./mod"` namespace ALIAS partner —
                // the name has both a TYPE meaning and a value NAMESPACE
                // meaning. If the partner aliases a module whose exports
                // include any runtime value, this name IS usable as a value.
                let has_namespace_alias_partner = self
                    .ctx
                    .alias_partner_for(target_binder, sym_id)
                    .is_some_and(|partner_id| {
                        target_binder
                            .get_symbol(partner_id)
                            .is_some_and(|partner_sym| {
                                partner_sym.has_any_flags(symbol_flags::ALIAS)
                                    && self.symbol_has_runtime_value_in_binder(
                                        target_binder,
                                        partner_id,
                                    )
                            })
                    });
                if has_namespace_alias_partner {
                    return false;
                }
                if sym.has_any_flags(PURE_TYPE) && !sym.has_any_flags(VALUE) {
                    // When `export type X = ...` merges with `export * as X from "..."`,
                    // the module_exports entry holds the TYPE_ALIAS but the binder records
                    // the value-providing ALIAS as an alias_partner. If such a partner
                    // exists, the merged name provides runtime value and is NOT type-only.
                    let has_value_partner =
                        self.ctx.alias_partners_contains(self.ctx.binder, sym_id);
                    // When the symbol also has ALIAS flag (e.g., `import * as B` merged
                    // with `interface B`), the alias part may provide runtime value. Don't
                    // declare type-only here — let the alias-chain-following logic below
                    // determine whether the alias target is actually type-only.
                    let alias_may_provide_value =
                        sym.has_any_flags(symbol_flags::ALIAS) && !sym.is_type_only;
                    if !has_value_partner && !alias_may_provide_value {
                        return true;
                    }
                }
                if sym.has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                    && !self.symbol_has_runtime_value_in_binder(target_binder, sym_id)
                {
                    // When the symbol also has ALIAS flag (e.g., `import { Enum }` merged
                    // with `namespace Enum { type Foo = ... }`), the alias may resolve to
                    // a value-providing target even though the namespace itself has no
                    // runtime value exports. Don't return type-only here.
                    if !sym.has_any_flags(symbol_flags::ALIAS) || sym.is_type_only {
                        return true;
                    }
                }
                let concrete_value = symbol_flags::VARIABLE
                    | symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::ENUM
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE;
                if !sym.has_any_flags(concrete_value)
                    && self.file_has_jsdoc_typedef_named(target_file_idx, export_name)
                {
                    return true;
                }
                // Follow import alias chains transitively, but only if the
                // symbol doesn't have a concrete runtime value binding.
                // A merged symbol like `import { A }` + `const A = 0` (VARIABLE)
                // provides a real value and overrides type-only from the import.
                // But `namespace A {}` (VALUE_MODULE) alone doesn't override.
                let concrete_value = symbol_flags::VARIABLE
                    | symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::ENUM;
                if sym.has_any_flags(symbol_flags::ALIAS) && !sym.has_any_flags(concrete_value) {
                    let mut visited_aliases = AliasCycleTracker::new();
                    if let Some(resolved_sym_id) =
                        self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                    {
                        for alias_id in &visited_aliases {
                            if target_binder
                                .get_symbol(alias_id)
                                .or_else(|| self.ctx.binder.get_symbol(alias_id))
                                .is_some_and(|alias_sym| alias_sym.is_type_only)
                            {
                                return true;
                            }
                        }

                        if let Some(resolved_sym) = target_binder
                            .get_symbol(resolved_sym_id)
                            .or_else(|| self.ctx.binder.get_symbol(resolved_sym_id))
                            && resolved_sym.has_any_flags(PURE_TYPE)
                            && !resolved_sym.has_any_flags(concrete_value)
                            // When the resolved symbol still has ALIAS flag (e.g.,
                            // `import * as B` merged with `interface B`), the alias
                            // side resolves to a namespace object that IS a value.
                            // Don't conclude type-only here — let the recursive
                            // is_export_type_only_in_file check below decide based
                            // on the actual import target.
                            && !resolved_sym.has_any_flags(symbol_flags::ALIAS)
                        {
                            return true;
                        }
                    }

                    if let Some(ref import_module) = sym.import_module {
                        let import_name = sym.import_name.as_deref().unwrap_or(&sym.escaped_name);
                        if self.is_export_type_only_in_file(
                            target_file_idx,
                            import_module,
                            import_name,
                            visited,
                        ) {
                            return true;
                        }
                    }
                }

                // `export default X` in ambient modules synthesizes a local ALIAS-like
                // `default` export symbol whose declaration is the identifier `X`.
                // When that target identifier is type-only (including namespace-only
                // symbols with no runtime value members), treat the default export as
                // type-only for cross-file import/value checks.
                if export_name == "default"
                    && sym.has_any_flags(symbol_flags::ALIAS)
                    && sym.import_module.is_none()
                    && let Some(target_decl_idx) = sym.primary_declaration()
                    && let Some(target_decl_node) = target_arena.get(target_decl_idx)
                    && let Some(target_ident) = target_arena.get_identifier(target_decl_node)
                {
                    let target_name = target_ident.escaped_text.as_str();
                    let target_sym_id = exports_table
                        .get(target_name)
                        .or_else(|| target_binder.file_locals.get(target_name));

                    if let Some(target_sym_id) = target_sym_id
                        && target_sym_id != sym_id
                    {
                        let target_sym_opt = target_binder
                            .get_symbol(target_sym_id)
                            .or_else(|| self.ctx.binder.get_symbol(target_sym_id));
                        if let Some(target_sym) = target_sym_opt {
                            const PURE_TYPE: u32 =
                                symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
                            const VALUE: u32 = symbol_flags::VARIABLE
                                | symbol_flags::FUNCTION
                                | symbol_flags::CLASS
                                | symbol_flags::ENUM
                                | symbol_flags::ENUM_MEMBER
                                | symbol_flags::VALUE_MODULE;

                            if target_sym.is_type_only
                                || (target_sym.has_any_flags(PURE_TYPE)
                                    && !target_sym.has_any_flags(VALUE))
                                || (target_sym.has_any_flags(
                                    symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE,
                                ) && !self.symbol_has_runtime_value_in_binder(
                                    target_binder,
                                    target_sym_id,
                                ))
                            {
                                return true;
                            }
                        }
                    }
                }
                // Direct export exists and is not type-only — don't check wildcard re-exports.
                return false;
            }
        }

        // Check named re-exports
        if let Some(file_reexports) = self
            .ctx
            .reexports_for_file(target_binder, &target_file_name)
            && let Some((source_module, original_name)) = file_reexports.get(export_name)
        {
            let name_to_lookup = original_name.as_deref().unwrap_or(export_name);
            return self.is_export_type_only_in_file(
                target_file_idx,
                source_module,
                name_to_lookup,
                visited,
            );
        }

        // Check wildcard re-exports (only if no direct export was found).
        // Two-pass approach: first check if any non-type-only wildcard provides
        // a value binding for the name (which overrides type-only from other
        // wildcards), then check type-only wildcards.
        if let Some(entries) = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
        {
            // Pass 1: Check non-type-only wildcards for value exports.
            // If a non-type-only `export *` re-exports the name AND the name is
            // not type-only in the source module, the value binding takes precedence
            // over any type-only wildcard (even if a `export type *` also has it).
            // Note: `name_exists_in_module_exports` only checks existence,
            // `is_export_type_only_in_file` checks the full type-only chain.
            for (source_module, source_is_type_only) in entries {
                if *source_is_type_only {
                    continue; // Skip type-only wildcards in pass 1
                }
                // Non-type-only wildcard: check if name exists as a value in source.
                // Use a separate visited set for the existence + type-only check
                // to avoid polluting the main cycle detection.
                let mut exists_visited = visited.clone();
                let exists_in_source = self.name_exists_in_module_exports(
                    target_file_idx,
                    source_module,
                    export_name,
                    &mut exists_visited,
                );
                if exists_in_source {
                    let mut type_only_visited = visited.clone();
                    let is_type_only_in_source = self.is_export_type_only_in_file(
                        target_file_idx,
                        source_module,
                        export_name,
                        &mut type_only_visited,
                    );
                    if !is_type_only_in_source {
                        // Value export found — name is NOT type-only
                        return false;
                    }
                }
            }

            // In JS files, `export type *` is a syntax error (TS8006), not a
            // semantic type-only marker. Skip type-only wildcard semantics for JS files.
            let target_is_js = target_file_name.ends_with(".js")
                || target_file_name.ends_with(".jsx")
                || target_file_name.ends_with(".mjs")
                || target_file_name.ends_with(".cjs");

            // Pass 2: Check type-only wildcards and transitive chains
            for (source_module, source_is_type_only) in entries {
                if *source_is_type_only {
                    // In JS files, `export type` is invalid syntax — don't treat as type-only
                    if target_is_js {
                        continue;
                    }
                    // Type-only wildcard: verify the name actually exists in the source
                    if self.name_exists_in_module_exports(
                        target_file_idx,
                        source_module,
                        export_name,
                        visited,
                    ) {
                        return true;
                    }
                    continue;
                }
                // Non-type-only wildcard: check for transitive type-only chains
                if self.is_export_type_only_in_file(
                    target_file_idx,
                    source_module,
                    export_name,
                    visited,
                ) {
                    return true;
                }
            }
        }

        false
    }
}
