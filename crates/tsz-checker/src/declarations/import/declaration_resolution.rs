//! Import resolution helpers, re-export cycle detection, conflict checking, and
//! module/file resolution utilities.
//!
//! Contains:
//! - `resolved_module_set_contains_specifier` — O(1) resolved-module lookup
//! - `check_reexport_chain_for_cycles` — circular `export *` guard
//! - `would_create_cycle` — import stack cycle test
//! - `resolve_import_via_target_binder` / `resolve_import_via_all_binders` — cross-binder lookup
//! - `resolve_import_in_file` — single-file export traversal
//! - `check_import_declaration_conflicts` — TS2440/TS2865 import-vs-local conflict checks
//! - `report_isolated_modules_import_conflicts` — isolated-modules conflict helper
//! - `resolved_via_directory_index` / `resolved_file_display_path` — TS2876 helpers
//! - `resolved_module_is_from_node_modules` / `module_target_is_typescript_input_file`
//! - `path_has_node_modules_segment` (free function)

use crate::context::is_declaration_file_name;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;

pub(crate) fn path_has_node_modules_segment(file_name: &str) -> bool {
    file_name
        .split(['/', '\\'])
        .any(|component| component == "node_modules")
}

impl<'a> CheckerState<'a> {
    pub(crate) fn resolved_module_set_contains_specifier(&self, module_name: &str) -> bool {
        self.ctx.resolved_modules.as_ref().is_some_and(|resolved| {
            crate::module_resolution::module_specifier_candidates(module_name)
                .iter()
                .any(|candidate| resolved.contains(candidate))
        })
    }

    // =========================================================================
    // Re-export Cycle Detection
    // =========================================================================

    /// Walk the re-export chain rooted at `module_name`, guarding against
    /// infinite recursion on circular chains.
    ///
    /// tsc does not emit a diagnostic for circular `export * from` chains —
    /// it simply treats the cycle as contributing no transitive exports. This
    /// walker exists purely to keep exported-symbol collection from spinning
    /// forever on self/mutually-referential packages (e.g. a typesVersions
    /// subfolder that re-exports from the package root and vice versa).
    pub(crate) fn check_reexport_chain_for_cycles(
        &mut self,
        module_name: &str,
        visited: &mut FxHashSet<String>,
    ) {
        if visited.contains(module_name) {
            return;
        }

        visited.insert(module_name.to_string());

        if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
            for source_module in source_modules {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        if let Some(reexports) = self.ctx.binder.reexports.get(module_name) {
            for (source_module, _) in reexports.values() {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        visited.remove(module_name);
    }

    /// Check if adding a module to the resolution path would create a cycle.
    pub(crate) fn would_create_cycle(&self, module: &str) -> bool {
        self.ctx
            .import_resolution_stack
            .contains(&module.to_string())
    }

    // =========================================================================
    // Re-export Resolution Helpers
    // =========================================================================

    /// Try to resolve an import through the target module's binder re-export chains.
    /// Traverses across binder boundaries by resolving each re-export source
    /// to its target file and checking that file's binder.
    pub(crate) fn resolve_import_via_target_binder(
        &self,
        module_name: &str,
        import_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> bool {
        let target_idx = if let Some(mode) = resolution_mode {
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                Some(mode),
            )
        } else {
            self.ctx.resolve_import_target(module_name)
        };
        if let Some(target_idx) = target_idx {
            let mut visited = rustc_hash::FxHashSet::default();
            return self.resolve_import_in_file(target_idx, import_name, &mut visited);
        }
        false
    }

    /// Try to resolve an import by searching binders' re-export chains.
    ///
    /// Uses `global_module_binder_index` for O(1) candidate lookup when available,
    /// falling back to an O(N) scan of all binders otherwise.
    pub(crate) fn resolve_import_via_all_binders(
        &self,
        module_name: &str,
        normalized: &str,
        import_name: &str,
    ) -> bool {
        let Some(all_binders) = &self.ctx.all_binders else {
            return false;
        };
        // Use global module binder index for O(1) candidate lookup.
        if let Some(ref idx) = self.ctx.global_module_binder_index {
            let candidate_indices = idx
                .get(module_name)
                .into_iter()
                .flatten()
                .chain(idx.get(normalized).into_iter().flatten());
            let mut seen = FxHashSet::default();
            for &binder_idx in candidate_indices {
                if !seen.insert(binder_idx) {
                    continue;
                }
                if let Some(binder) = all_binders.get(binder_idx)
                    && (binder
                        .resolve_import_if_needed_public(module_name, import_name)
                        .is_some()
                        || binder
                            .resolve_import_if_needed_public(normalized, import_name)
                            .is_some())
                {
                    return true;
                }
            }
            return false;
        }
        // Fallback: O(N) scan when index not built.
        for binder in all_binders.iter() {
            if binder
                .resolve_import_if_needed_public(module_name, import_name)
                .is_some()
                || binder
                    .resolve_import_if_needed_public(normalized, import_name)
                    .is_some()
            {
                return true;
            }
        }
        false
    }

    /// Resolve an import by checking a specific file's exports and following
    /// re-export chains across binder boundaries. Each file has its own binder
    /// in multi-file mode, so we traverse wildcard/named re-exports by resolving
    /// each source specifier to its target file and checking that file's binder.
    pub(crate) fn resolve_import_in_file(
        &self,
        file_idx: usize,
        import_name: &str,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) -> bool {
        if !visited.insert(file_idx) {
            return false; // Cycle detection
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(file_idx) else {
            return false;
        };

        let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };

        // Check direct exports
        if let Some(exports) = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            && exports.has(import_name)
        {
            return true;
        }

        // Check named re-exports
        if let Some(reexports) = self
            .ctx
            .reexports_for_file(target_binder, &target_file_name)
            && let Some((source_module, original_name)) = reexports.get(import_name)
        {
            let name = original_name.as_deref().unwrap_or(import_name);
            if let Some(source_idx) = self
                .ctx
                .resolve_import_target_from_file(file_idx, source_module)
                && self.resolve_import_in_file(source_idx, name, visited)
            {
                return true;
            }
        }

        // Check wildcard re-exports
        if let Some(source_modules) = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
        {
            let source_modules = source_modules.clone();
            for source_module in &source_modules {
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                    && self.resolve_import_in_file(source_idx, import_name, visited)
                {
                    return true;
                }
            }
        }

        false
    }

    pub(crate) fn check_import_declaration_conflicts(
        &mut self,
        stmt_idx: NodeIndex,
        clause_idx: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };

        let mut bindings_to_check = Vec::new();

        if clause.name.is_some() {
            bindings_to_check.push((clause_idx, clause.name, clause.name));
        }

        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings)
        {
            if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                if let Some(ns) = self.ctx.arena.get_named_imports(bindings_node)
                    && ns.name.is_some()
                {
                    bindings_to_check.push((clause.named_bindings, ns.name, ns.name));
                }
            } else if bindings_node.kind == syntax_kind_ext::NAMED_IMPORTS
                && let Some(named) = self.ctx.arena.get_named_imports(bindings_node)
            {
                for &spec_idx in &named.elements.nodes {
                    if let Some(spec_node) = self.ctx.arena.get(spec_idx)
                        && let Some(spec) = self.ctx.arena.get_specifier(spec_node)
                    {
                        let name_idx = if spec.name.is_some() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        let diagnostic_name_idx = if spec.property_name.is_some() {
                            // For aliased named imports (`import { x as y }`), tsc
                            // anchors TS2440 at the imported name (`x`), while the
                            // message still references the local binding (`y`).
                            spec.property_name
                        } else {
                            name_idx
                        };
                        if name_idx.is_some() {
                            bindings_to_check.push((spec_idx, name_idx, diagnostic_name_idx));
                        }
                    }
                }
            }
        }

        for (binding_node_idx, name_idx, diagnostic_name_idx) in bindings_to_check {
            if let Some(name_node) = self.ctx.arena.get(name_idx)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.clone();
                let sym_id_opt = self
                    .ctx
                    .binder
                    .node_symbols
                    .get(&binding_node_idx.0)
                    .copied();
                if let Some(sym_id) = sym_id_opt {
                    let mut has_conflict = false;
                    if let Some(sym) = self.ctx.binder.symbols.get(sym_id) {
                        if sym.is_type_only {
                            continue;
                        }

                        // Fast path: if there is no other local declaration merged into
                        // this symbol and no separate same-name symbol in the current file,
                        // there is nothing for this import to conflict with. Avoid resolving
                        // the import alias just to discover the absence of a candidate.
                        let has_merged_local_candidate = sym.declarations.iter().any(|&decl_idx| {
                            decl_idx != binding_node_idx
                                && decl_idx != clause_idx
                                && decl_idx != stmt_idx
                                && self.ctx.binder.node_symbols.contains_key(&decl_idx.0)
                        });
                        let has_same_name_candidate = self
                            .ctx
                            .binder
                            .symbols
                            .find_all_by_name(&name)
                            .iter()
                            .any(|&other_sym_id| other_sym_id != sym_id);
                        if !has_merged_local_candidate && !has_same_name_candidate {
                            continue;
                        }

                        let mut import_has_value = false;
                        let mut import_has_type = false;
                        let mut visited = AliasCycleTracker::new();
                        if let Some(resolved_id) = self.resolve_alias_symbol(sym_id, &mut visited)
                            // When resolve_alias_symbol returns the SAME symbol, it
                            // means resolution failed (e.g. unresolved external module).
                            // The symbol's flags include merged local declarations,
                            // which would give a false positive.
                            && resolved_id != sym_id
                            && let Some(resolved_sym) = self
                                .ctx
                                .binder
                                .get_symbol_with_libs(resolved_id, &self.get_lib_binders())
                        {
                            let mut has_value = resolved_sym
                                .has_any_flags(symbol_flags::VALUE | symbol_flags::EXPORT_VALUE);
                            if has_value
                                && resolved_sym.has_any_flags(symbol_flags::VALUE_MODULE)
                                && !resolved_sym.has_any_flags(
                                    symbol_flags::VALUE & !symbol_flags::VALUE_MODULE,
                                )
                            {
                                let mut any_instantiated = false;
                                for &decl_idx in &resolved_sym.declarations {
                                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                                        continue;
                                    };
                                    if decl_node.kind
                                        == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION
                                    {
                                        if self.is_namespace_declaration_instantiated(decl_idx) {
                                            any_instantiated = true;
                                            break;
                                        }
                                    } else {
                                        any_instantiated = true;
                                        break;
                                    }
                                }
                                has_value = any_instantiated;
                            }
                            import_has_value = has_value;
                            // Check if the imported symbol carries type semantics
                            // (e.g. enum, class, interface). When it does, local type
                            // aliases or interfaces with the same name conflict.
                            if resolved_sym.has_any_flags(symbol_flags::TYPE) {
                                import_has_type = true;
                            }
                            if resolved_sym.has_any_flags(symbol_flags::ALIAS)
                                && sym.import_module.is_some()
                                && sym.import_name.is_none()
                            {
                                import_has_value = true;
                            }
                        }

                        // Cross-file fallback: when resolve_alias_symbol returns the alias
                        // itself (can't resolve cross-file), check the exported symbol's
                        // flags directly in the target file's binder.
                        if (!import_has_value || !import_has_type)
                            && let Some(ref module_name) = sym.import_module
                        {
                            let export_name = sym.import_name.as_deref().unwrap_or(&name);
                            // Try declared modules (module_exports)
                            // Use global_module_binder_index for O(1) lookup instead of O(N) binder scan
                            if let Some(binders) = &self.ctx.all_binders {
                                let candidate_indices = self
                                    .ctx
                                    .global_module_binder_index
                                    .as_ref()
                                    .and_then(|idx| idx.get(module_name.as_str()));
                                if let Some(indices) = candidate_indices {
                                    for &binder_idx in indices {
                                        if let Some(binder) = binders.get(binder_idx)
                                            && let Some(exports) =
                                                self.ctx.module_exports_for_module(
                                                    binder,
                                                    module_name.as_str(),
                                                )
                                            && let Some(target_sym_id) = exports.get(export_name)
                                            && let Some(target_sym) =
                                                binder.symbols.get(target_sym_id)
                                        {
                                            if target_sym.has_any_flags(
                                                symbol_flags::VALUE | symbol_flags::EXPORT_VALUE,
                                            ) {
                                                import_has_value = true;
                                            }
                                            if target_sym.has_any_flags(symbol_flags::TYPE) {
                                                import_has_type = true;
                                            }
                                            if import_has_value {
                                                break;
                                            }
                                        }
                                    }
                                } else {
                                    for binder in binders.iter() {
                                        if let Some(exports) = self
                                            .ctx
                                            .module_exports_for_module(binder, module_name.as_str())
                                            && let Some(target_sym_id) = exports.get(export_name)
                                            && let Some(target_sym) =
                                                binder.symbols.get(target_sym_id)
                                        {
                                            if target_sym.has_any_flags(
                                                symbol_flags::VALUE | symbol_flags::EXPORT_VALUE,
                                            ) {
                                                import_has_value = true;
                                            }
                                            if target_sym.has_any_flags(symbol_flags::TYPE) {
                                                import_has_type = true;
                                            }
                                            if import_has_value {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            // Try regular file exports: follow re-export chains
                            // (module_exports → named re-exports → wildcard re-exports)
                            // to find the actual exported symbol.  Using file_locals directly
                            // would pick up globals leaked by create_binder_from_bound_file.
                            if (!import_has_value || !import_has_type)
                                && let Some(target_idx) =
                                    self.ctx.resolve_import_target(module_name)
                            {
                                let mut visited = FxHashSet::default();
                                if let Some((resolved_sym_id, resolved_file_idx)) = self
                                    .resolve_export_in_file(target_idx, export_name, &mut visited)
                                {
                                    let resolved_binder =
                                        self.ctx.get_binder_for_file(resolved_file_idx);
                                    if let Some(resolved_sym) =
                                        resolved_binder.and_then(|b| b.symbols.get(resolved_sym_id))
                                    {
                                        if resolved_sym.has_any_flags(
                                            symbol_flags::VALUE | symbol_flags::EXPORT_VALUE,
                                        ) {
                                            import_has_value = true;
                                        }
                                        if resolved_sym.has_any_flags(symbol_flags::TYPE) {
                                            import_has_type = true;
                                        }
                                        // Non-type-only re-export aliases forward values
                                        if !import_has_value
                                            && resolved_sym.has_any_flags(symbol_flags::ALIAS)
                                            && !resolved_sym.is_type_only
                                        {
                                            import_has_value = true;
                                        }
                                        // When a type alias shadows an import alias,
                                        // follow alias_partners to the partner ALIAS
                                        // and check its import chain for value semantics.
                                        if !import_has_value
                                            && resolved_sym.has_any_flags(symbol_flags::TYPE_ALIAS)
                                            && !resolved_sym.is_type_only
                                            && let Some(resolved_binder) =
                                                self.ctx.get_binder_for_file(resolved_file_idx)
                                            && let Some(partner_id) = self
                                                .ctx
                                                .alias_partner_for(resolved_binder, resolved_sym_id)
                                            && let Some(partner) =
                                                resolved_binder.symbols.get(partner_id)
                                            && partner.has_any_flags(symbol_flags::ALIAS)
                                            && !partner.is_type_only
                                            && let Some(ref src_module) = partner.import_module
                                        {
                                            let src_name = partner
                                                .import_name
                                                .as_deref()
                                                .unwrap_or(export_name);
                                            if let Some(src_idx) =
                                                self.ctx.resolve_import_target_from_file(
                                                    resolved_file_idx,
                                                    src_module,
                                                )
                                            {
                                                let mut inner_visited = FxHashSet::default();
                                                if let Some((src_sym_id, src_file_idx)) = self
                                                    .resolve_export_in_file(
                                                        src_idx,
                                                        src_name,
                                                        &mut inner_visited,
                                                    )
                                                    && let Some(src_binder) =
                                                        self.ctx.get_binder_for_file(src_file_idx)
                                                    && let Some(src_sym) =
                                                        src_binder.symbols.get(src_sym_id)
                                                    && src_sym.has_any_flags(
                                                        symbol_flags::VALUE
                                                            | symbol_flags::EXPORT_VALUE,
                                                    )
                                                {
                                                    import_has_value = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Namespace imports (`import * as X`) always create a
                        // value binding (the module namespace object), even when
                        // the target module can't be resolved.
                        if !import_has_value
                            && let Some(binding_node) = self.ctx.arena.get(binding_node_idx)
                            && binding_node.kind == syntax_kind_ext::NAMESPACE_IMPORT
                        {
                            import_has_value = true;
                        }

                        if !import_has_value {
                            // Even when the imported target carries no value,
                            // there are two isolated-modules-specific checks:
                            //
                            // (a) TS2865: imported `T` is type-only at target,
                            //     but the local file has a value declaration
                            //     called `T` (`const T = 0`). Under
                            //     isolatedModules the import would be erased
                            //     by the transpiler and the local value would
                            //     replace it. tsc requires `import type` here.
                            //
                            // (b) TS2440: imported `T` is type-only at target,
                            //     and the local file has a TYPE declaration
                            //     called `T` (`type T = number`). Both bind
                            //     the same type-name slot.
                            self.report_isolated_modules_import_conflicts(
                                stmt_idx,
                                binding_node_idx,
                                clause_idx,
                                diagnostic_name_idx,
                                &name,
                                sym_id,
                                import_has_type,
                            );
                            continue;
                        }

                        // Use the import STATEMENT's enclosing scope — the scope
                        // the import lives in (e.g. module scope).  We avoid using
                        // the import-specifier's scope because `find_enclosing_scope`
                        // may differ from the statement scope when the specifier is
                        // inside a NamedImports node that happens to be scope-creating.
                        let import_scope = self
                            .ctx
                            .binder
                            .find_enclosing_scope(self.ctx.arena, stmt_idx);

                        // Check 1: merged declarations on the import's own symbol.
                        has_conflict = sym.declarations.iter().any(|&decl_idx| {
                            if decl_idx == binding_node_idx
                                || decl_idx == clause_idx
                                || decl_idx == stmt_idx
                            {
                                return false;
                            }
                            let is_current_file_decl =
                                self.ctx.binder.node_symbols.contains_key(&decl_idx.0);
                            if !is_current_file_decl {
                                return false;
                            }
                            // Skip declarations inside module augmentations
                            // (`declare module "./foo" { ... }`).  The binder may
                            // not create a separate scope for the augmentation block,
                            // so the scope check alone can't detect this.
                            if self.is_inside_module_augmentation(decl_idx) {
                                return false;
                            }
                            // `declare global { ... }` injects declarations into the
                            // global scope, not the module scope the import lives in.
                            // Those declarations must not collide with module imports.
                            if self.is_inside_global_augmentation(decl_idx) {
                                return false;
                            }
                            // Scope check: the declaration must be in the same
                            // logical scope as the import.  We compare scopes by
                            // checking if they are the same ScopeId OR if they
                            // share the same container symbol (merged namespace
                            // blocks create separate scopes but share one symbol).
                            // Use the PARENT's scope for scope-creating nodes
                            // (e.g. function/class declarations create a body
                            // scope, but they *live in* the parent scope).
                            let decl_containing_scope =
                                self.ctx.arena.get_extended(decl_idx).and_then(|ext| {
                                    let parent = ext.parent;
                                    if parent.is_some() {
                                        self.ctx.binder.find_enclosing_scope(self.ctx.arena, parent)
                                    } else {
                                        self.ctx
                                            .binder
                                            .find_enclosing_scope(self.ctx.arena, decl_idx)
                                    }
                                });
                            let in_same_scope = match (import_scope, decl_containing_scope) {
                                (Some(a), Some(b)) if a == b => true,
                                (Some(a), Some(b)) => {
                                    // Merged namespace: check if both scopes'
                                    // container nodes map to the same symbol.
                                    let sym_a =
                                        self.ctx.binder.scopes.get(a.0 as usize).and_then(|s| {
                                            self.ctx.binder.node_symbols.get(&s.container_node.0)
                                        });
                                    let sym_b =
                                        self.ctx.binder.scopes.get(b.0 as usize).and_then(|s| {
                                            self.ctx.binder.node_symbols.get(&s.container_node.0)
                                        });
                                    sym_a.is_some() && sym_a == sym_b
                                }
                                _ => true,
                            };
                            if !in_same_scope {
                                return false;
                            }

                            // `export as namespace X` only binds a global
                            // namespace alias, never a local module binding.
                            if self.decl_is_namespace_export_declaration(decl_idx) {
                                return false;
                            }
                            if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                if matches!(
                                    decl_node.kind,
                                    syntax_kind_ext::IMPORT_CLAUSE
                                        | syntax_kind_ext::NAMESPACE_IMPORT
                                        | syntax_kind_ext::IMPORT_SPECIFIER
                                        | syntax_kind_ext::NAMED_IMPORTS
                                        | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                                        | syntax_kind_ext::IMPORT_DECLARATION
                                        // Re-exports (`export { x } from "./b"`) don't
                                        // introduce local bindings, so they must not
                                        // conflict with imports.
                                        | syntax_kind_ext::EXPORT_SPECIFIER
                                        | syntax_kind_ext::EXPORT_DECLARATION
                                        | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                                ) {
                                    return false;
                                }
                                // Type aliases and interfaces live in the type declaration
                                // space. They only conflict with imports that also carry
                                // type semantics (e.g. enums, classes).
                                if !import_has_type
                                    && matches!(
                                        decl_node.kind,
                                        syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                            | syntax_kind_ext::INTERFACE_DECLARATION
                                    )
                                {
                                    return false;
                                }
                                // Non-import, non-type local declarations (var, function,
                                // class, namespace, enum) conflict with value imports.
                                // Type declarations conflict when the import has type meaning.
                                true
                            } else {
                                false
                            }
                        });

                        // Check 2: separate symbols with the same name (binder may
                        // create distinct symbols instead of merging declarations).
                        if !has_conflict {
                            let all_symbols = self.ctx.binder.symbols.find_all_by_name(&name);
                            for &other_sym_id in all_symbols {
                                if other_sym_id == sym_id {
                                    continue;
                                }
                                if let Some(other_sym) = self.ctx.binder.symbols.get(other_sym_id) {
                                    // Skip if the other symbol is purely an alias (another import)
                                    if other_sym.has_any_flags(symbol_flags::ALIAS)
                                        && !other_sym.has_any_flags(!symbol_flags::ALIAS)
                                    {
                                        continue;
                                    }
                                    // Skip type-only symbols (type aliases, interfaces) — they
                                    // live in the type declaration space and don't conflict
                                    // with value-only imports. When the import also carries
                                    // type semantics (e.g. enum, class), they DO conflict.
                                    if !import_has_type {
                                        let type_only_flags = symbol_flags::TYPE_ALIAS
                                            | symbol_flags::INTERFACE
                                            | symbol_flags::TYPE_PARAMETER;
                                        if other_sym.has_any_flags(type_only_flags)
                                            && !other_sym.has_any_flags(symbol_flags::VALUE)
                                        {
                                            continue;
                                        }
                                    }
                                    // Must have a declaration in the same scope
                                    let decl_in_same_scope =
                                        other_sym.declarations.iter().any(|&decl_idx| {
                                            let decl_containing =
                                                self.ctx.arena.get_extended(decl_idx).and_then(
                                                    |ext| {
                                                        let parent = ext.parent;
                                                        if parent.is_some() {
                                                            self.ctx.binder.find_enclosing_scope(
                                                                self.ctx.arena,
                                                                parent,
                                                            )
                                                        } else {
                                                            self.ctx.binder.find_enclosing_scope(
                                                                self.ctx.arena,
                                                                decl_idx,
                                                            )
                                                        }
                                                    },
                                                );

                                            match (import_scope, decl_containing) {
                                                (Some(a), Some(b)) => a == b,
                                                _ => true,
                                            }
                                        });
                                    if !decl_in_same_scope {
                                        continue;
                                    }
                                    // Must be in the current file and not an
                                    // import/export specifier (re-exports like
                                    // `export { x } from "./b"` don't create local
                                    // bindings and must not conflict with imports).
                                    let has_local_decl =
                                        other_sym.declarations.iter().any(|&decl_idx| {
                                            if self.ctx.binder.node_symbols.get(&decl_idx.0)
                                                != Some(&other_sym_id)
                                            {
                                                return false;
                                            }
                                            // `declare module "..." { ... }` augmentation members
                                            // merge into the augmented module's export table, not
                                            // the augmenting file's scope, so they can't conflict
                                            // with an import. Mirrors the Check 1 skip above.
                                            if self.is_inside_module_augmentation(decl_idx) {
                                                return false;
                                            }
                                            // `declare global { ... }` places declarations in
                                            // the global scope; they can't conflict with an
                                            // import living in the enclosing module scope.
                                            if self.is_inside_global_augmentation(decl_idx) {
                                                return false;
                                            }
                                            // `export as namespace X` declares a global
                                            // namespace alias for the module. It does not
                                            // introduce a local binding, so it must not
                                            // collide with a module-scope import. The binder
                                            // may point at the identifier inside the
                                            // declaration, so check both the node itself and
                                            // its immediate parent.
                                            if self.decl_is_namespace_export_declaration(decl_idx)
                                            {
                                                return false;
                                            }
                                            if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                                if matches!(
                                                    decl_node.kind,
                                                    syntax_kind_ext::EXPORT_SPECIFIER
                                                        | syntax_kind_ext::EXPORT_DECLARATION
                                                        | syntax_kind_ext::IMPORT_CLAUSE
                                                        | syntax_kind_ext::NAMESPACE_IMPORT
                                                        | syntax_kind_ext::IMPORT_SPECIFIER
                                                        | syntax_kind_ext::NAMED_IMPORTS
                                                        | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                                                        | syntax_kind_ext::IMPORT_DECLARATION
                                                        | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                                                ) {
                                                    return false;
                                                }
                                                // Type declarations only conflict when
                                                // the import also carries type semantics.
                                                if !import_has_type
                                                    && matches!(
                                                        decl_node.kind,
                                                        syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                                            | syntax_kind_ext::INTERFACE_DECLARATION
                                                    )
                                                {
                                                    return false;
                                                }
                                                true
                                            } else {
                                                false
                                            }
                                        });
                                    if has_local_decl {
                                        has_conflict = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    if has_conflict {
                        let message = format_message(
                                diagnostic_messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                                &[&name],
                            );
                        self.error_at_node(
                                diagnostic_name_idx,
                                &message,
                                diagnostic_codes::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                            );
                        // Record so TS2456 can be suppressed for type aliases
                        // whose apparent circularity is caused by this conflict.
                        self.ctx.import_conflict_names.insert(name.clone());
                    }
                }
            }
        }
    }

    /// Helper for `check_import_declaration_conflicts` covering the
    /// type-only-import case. When the imported target carries no value
    /// semantics, we still need two isolatedModules-specific diagnostics:
    ///
    /// - **TS2440**: a local TYPE declaration with the same name. tsc treats
    ///   this as `excludedMeanings & Type` overlap.
    /// - **TS2865**: a local VALUE declaration with the same name. tsc emits
    ///   this only under isolatedModules to flag that the transpiler would
    ///   erase the import and pick the local value instead.
    ///
    /// Both must respect the same scope/declaration filters as the main
    /// conflict check (skip module-augmentation/global-augmentation decls,
    /// skip re-export specifiers, etc.).
    fn report_isolated_modules_import_conflicts(
        &mut self,
        stmt_idx: NodeIndex,
        binding_node_idx: NodeIndex,
        clause_idx: NodeIndex,
        diagnostic_name_idx: NodeIndex,
        name: &str,
        sym_id: tsz_binder::SymbolId,
        import_has_type: bool,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        // The import declaration node at this point is for a regular
        // ImportDeclaration / ImportSpecifier — the import-equals path is
        // handled elsewhere. Skip when the import itself is type-only.
        let import_is_type_only_syntax = self
            .ctx
            .arena
            .get(clause_idx)
            .and_then(|n| self.ctx.arena.get_import_clause(n))
            .map(|c| c.is_type_only)
            .unwrap_or(false);
        if import_is_type_only_syntax {
            return;
        }
        // ImportSpecifier-level `type` modifier (`import { type T }`).
        if self
            .ctx
            .arena
            .get(binding_node_idx)
            .and_then(|n| self.ctx.arena.get_specifier(n))
            .is_some_and(|spec| spec.is_type_only)
        {
            return;
        }

        // Already-reported conflicts: avoid double-reporting when
        // import_conflict_names was set by another path.
        if self.ctx.import_conflict_names.contains(name) {
            return;
        }

        // Walk other same-name local symbols and figure out whether any
        // carry Value or pure-Type meaning.
        let import_scope = self
            .ctx
            .binder
            .find_enclosing_scope(self.ctx.arena, stmt_idx);
        let mut local_has_value = false;
        let mut local_has_pure_type = false;

        // Look at merged decls on the import's own symbol.
        if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
            for &decl_idx in &sym.declarations {
                if decl_idx == binding_node_idx || decl_idx == clause_idx || decl_idx == stmt_idx {
                    continue;
                }
                if !self.ctx.binder.node_symbols.contains_key(&decl_idx.0) {
                    continue;
                }
                if self.is_inside_module_augmentation(decl_idx) {
                    continue;
                }
                if self.is_inside_global_augmentation(decl_idx) {
                    continue;
                }
                if self.decl_is_namespace_export_declaration(decl_idx) {
                    continue;
                }
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if matches!(
                    decl_node.kind,
                    syntax_kind_ext::IMPORT_CLAUSE
                        | syntax_kind_ext::NAMESPACE_IMPORT
                        | syntax_kind_ext::IMPORT_SPECIFIER
                        | syntax_kind_ext::NAMED_IMPORTS
                        | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        | syntax_kind_ext::IMPORT_DECLARATION
                        | syntax_kind_ext::EXPORT_SPECIFIER
                        | syntax_kind_ext::EXPORT_DECLARATION
                        | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                ) {
                    continue;
                }
                // Same-scope check.
                let decl_containing_scope = self.ctx.arena.get_extended(decl_idx).and_then(|ext| {
                    let parent = ext.parent;
                    if parent.is_some() {
                        self.ctx.binder.find_enclosing_scope(self.ctx.arena, parent)
                    } else {
                        self.ctx
                            .binder
                            .find_enclosing_scope(self.ctx.arena, decl_idx)
                    }
                });
                let in_same_scope =
                    match (import_scope, decl_containing_scope) {
                        (Some(a), Some(b)) if a == b => true,
                        (Some(a), Some(b)) => {
                            let sym_a = self.ctx.binder.scopes.get(a.0 as usize).and_then(|s| {
                                self.ctx.binder.node_symbols.get(&s.container_node.0)
                            });
                            let sym_b = self.ctx.binder.scopes.get(b.0 as usize).and_then(|s| {
                                self.ctx.binder.node_symbols.get(&s.container_node.0)
                            });
                            sym_a.is_some() && sym_a == sym_b
                        }
                        _ => true,
                    };
                if !in_same_scope {
                    continue;
                }
                match decl_node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::ENUM_DECLARATION
                    | syntax_kind_ext::VARIABLE_DECLARATION
                    | syntax_kind_ext::VARIABLE_STATEMENT => {
                        local_has_value = true;
                    }
                    syntax_kind_ext::MODULE_DECLARATION
                        if self.is_namespace_declaration_instantiated(decl_idx) =>
                    {
                        local_has_value = true;
                    }
                    syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    | syntax_kind_ext::INTERFACE_DECLARATION => {
                        local_has_pure_type = true;
                    }
                    _ => {}
                }
            }
        }

        // Look at separate same-name symbols (binder may keep them split
        // across imports vs locals via alias_partners).
        if !local_has_value || !local_has_pure_type {
            let all_symbols: Vec<tsz_binder::SymbolId> =
                self.ctx.binder.symbols.find_all_by_name(name).to_vec();
            for other_sym_id in all_symbols {
                if other_sym_id == sym_id {
                    continue;
                }
                let Some(other_sym) = self.ctx.binder.symbols.get(other_sym_id) else {
                    continue;
                };
                // Skip purely-alias symbols (other imports).
                if other_sym.has_any_flags(symbol_flags::ALIAS)
                    && !other_sym.has_any_flags(!symbol_flags::ALIAS)
                {
                    continue;
                }
                // Same-scope filter.
                let other_in_same_scope = other_sym.declarations.iter().any(|&decl_idx| {
                    let decl_containing = self.ctx.arena.get_extended(decl_idx).and_then(|ext| {
                        let parent = ext.parent;
                        if parent.is_some() {
                            self.ctx.binder.find_enclosing_scope(self.ctx.arena, parent)
                        } else {
                            self.ctx
                                .binder
                                .find_enclosing_scope(self.ctx.arena, decl_idx)
                        }
                    });
                    match (import_scope, decl_containing) {
                        (Some(a), Some(b)) => a == b,
                        _ => true,
                    }
                });
                if !other_in_same_scope {
                    continue;
                }
                let has_local_decl = other_sym.declarations.iter().any(|&decl_idx| {
                    if self.ctx.binder.node_symbols.get(&decl_idx.0) != Some(&other_sym_id) {
                        return false;
                    }
                    if self.is_inside_module_augmentation(decl_idx) {
                        return false;
                    }
                    if self.is_inside_global_augmentation(decl_idx) {
                        return false;
                    }
                    if self.decl_is_namespace_export_declaration(decl_idx) {
                        return false;
                    }
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        return false;
                    };
                    !matches!(
                        decl_node.kind,
                        syntax_kind_ext::EXPORT_SPECIFIER
                            | syntax_kind_ext::EXPORT_DECLARATION
                            | syntax_kind_ext::IMPORT_CLAUSE
                            | syntax_kind_ext::NAMESPACE_IMPORT
                            | syntax_kind_ext::IMPORT_SPECIFIER
                            | syntax_kind_ext::NAMED_IMPORTS
                            | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                            | syntax_kind_ext::IMPORT_DECLARATION
                            | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                    )
                });
                if !has_local_decl {
                    continue;
                }
                if other_sym.has_any_flags(symbol_flags::VALUE | symbol_flags::EXPORT_VALUE) {
                    local_has_value = true;
                }
                let pure_type_flags = symbol_flags::TYPE_ALIAS | symbol_flags::INTERFACE;
                if other_sym.has_any_flags(pure_type_flags)
                    && !other_sym.has_any_flags(symbol_flags::VALUE)
                {
                    local_has_pure_type = true;
                }
            }
        }

        // TS2440: local Type collides with imported Type.
        if local_has_pure_type && import_has_type {
            let message = format_message(
                diagnostic_messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                &[name],
            );
            self.error_at_node(
                diagnostic_name_idx,
                &message,
                diagnostic_codes::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
            );
            self.ctx.import_conflict_names.insert(name.to_string());
            return;
        }

        // TS2865: local Value collides with imported type-only target under
        // isolatedModules. Not under verbatimModuleSyntax — that case is
        // already covered by the existing TS1361/TS1362 imports-of-types
        // diagnostics.
        if local_has_value
            && self.ctx.compiler_options.isolated_modules
            && !self.ctx.compiler_options.verbatim_module_syntax
        {
            let message = format_message(
                diagnostic_messages::IMPORT_CONFLICTS_WITH_LOCAL_VALUE_SO_MUST_BE_DECLARED_WITH_A_TYPE_ONLY_IMPORT_WH,
                &[name],
            );
            // tsc anchors TS2865 at the whole import specifier node, not
            // just the imported name. Preserve that for fingerprint parity.
            self.error_at_node(
                binding_node_idx,
                &message,
                diagnostic_codes::IMPORT_CONFLICTS_WITH_LOCAL_VALUE_SO_MUST_BE_DECLARED_WITH_A_TYPE_ONLY_IMPORT_WH,
            );
            self.ctx.import_conflict_names.insert(name.to_string());
        }
    }

    /// Returns `true` if `specifier` resolves via directory-index probing rather
    /// than direct TS-extension file matching.
    ///
    /// This mirrors tsc's `!resolvedModule.resolvedUsingTsExtension`:
    /// if the specifier is `./foo.ts` but the resolved file is
    /// `foo.ts/index.ts`, the TS extension in the specifier was NOT used to
    /// find the file — directory probing found it instead.
    pub(crate) fn resolved_via_directory_index(&self, specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(specifier) else {
            return false;
        };
        let Some(arenas) = self.ctx.all_arenas.as_ref() else {
            return false;
        };
        let Some(target_arena) = arenas.get(target_idx) else {
            return false;
        };
        let Some(sf) = target_arena.source_files.first() else {
            return false;
        };
        // Extract the stem (without extension) from the specifier basename.
        let spec_file = specifier
            .rsplit_once('/')
            .map_or(specifier, |(_, file)| file);
        let spec_stem = spec_file.rfind('.').map_or(spec_file, |i| &spec_file[..i]);
        // Extract the stem from the resolved file's basename.
        // For declaration files like "foo.d.ts", strip all declaration
        // suffixes to get the base stem "foo".
        let resolved_file = sf
            .file_name
            .rsplit_once('/')
            .map_or(sf.file_name.as_str(), |(_, file)| file);
        let resolved_stem = resolved_file
            .strip_suffix(".d.ts")
            .or_else(|| resolved_file.strip_suffix(".d.mts"))
            .or_else(|| resolved_file.strip_suffix(".d.cts"))
            .or_else(|| resolved_file.rfind('.').map(|i| &resolved_file[..i]))
            .unwrap_or(resolved_file);
        // If the stems match, the resolution used the TS extension directly
        // (e.g., ./obj.ts → obj.d.ts). If stems differ, it went through
        // directory probing (e.g., ./foo.ts → foo.ts/index.d.ts).
        resolved_stem != spec_stem
    }

    /// Returns a relative display path for the resolved target of `specifier`,
    /// suitable for the TS2876 diagnostic message argument.
    pub(crate) fn resolved_file_display_path(&self, specifier: &str) -> String {
        let Some(target_idx) = self.ctx.resolve_import_target(specifier) else {
            return specifier.to_string();
        };
        let Some(arenas) = self.ctx.all_arenas.as_ref() else {
            return specifier.to_string();
        };
        let Some(target_arena) = arenas.get(target_idx) else {
            return specifier.to_string();
        };
        let Some(sf) = target_arena.source_files.first() else {
            return specifier.to_string();
        };
        // Return a relative path with "./" prefix, matching tsc's output format.
        let resolved = &sf.file_name;
        if resolved.starts_with("./") || resolved.starts_with("../") {
            resolved.clone()
        } else {
            format!("./{resolved}")
        }
    }

    /// Returns `true` if `specifier` resolves to a file inside `node_modules/`.
    /// Mirrors tsc's `isExternalLibraryImport` — external library imports should
    /// not trigger TS2877 rewrite-extension warnings.
    pub(crate) fn resolved_module_is_from_node_modules(&self, specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(specifier) else {
            return false;
        };
        let Some(arenas) = self.ctx.all_arenas.as_ref() else {
            return false;
        };
        let Some(target_arena) = arenas.get(target_idx) else {
            return false;
        };
        let Some(source_file) = target_arena.source_files.first() else {
            return false;
        };
        path_has_node_modules_segment(&source_file.file_name)
    }

    /// Returns `true` if `specifier` resolves to a non-declaration TypeScript input
    /// file (`.ts`, `.tsx`, `.mts`, `.cts`) that can participate in emit rewriting.
    pub(crate) fn module_target_is_typescript_input_file(&self, specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(specifier) else {
            return false;
        };
        let Some(arenas) = self.ctx.all_arenas.as_ref() else {
            return false;
        };
        let Some(target_arena) = arenas.get(target_idx) else {
            return false;
        };
        let Some(source_file) = target_arena.source_files.first() else {
            return false;
        };
        let file_name = source_file.file_name.as_str();
        if is_declaration_file_name(file_name) {
            return false;
        }

        file_name.ends_with(".ts")
            || file_name.ends_with(".tsx")
            || file_name.ends_with(".mts")
            || file_name.ends_with(".cts")
    }
}
