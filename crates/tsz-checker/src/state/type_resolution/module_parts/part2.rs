impl<'a> CheckerState<'a> {
    /// Emit TS2307 error for a module that cannot be found.
    ///
    /// This function emits a "Cannot find module" error with the module specifier
    /// and attempts to report the error at the import declaration node if available.
    pub(crate) fn emit_module_not_found_error(
        &mut self,
        module_specifier: &str,
        decl_node: NodeIndex,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        // Only emit if report_unresolved_imports is enabled
        // (CLI driver handles module resolution in multi-file mode)
        if !self.ctx.report_unresolved_imports {
            return;
        }

        // For import declarations, defer to check_import_declaration / check_import_equals_declaration
        // which have accurate module specifier positions and handle special cases (e.g., TS1147 for
        // imports in namespaces). This function may be called during type resolution with incorrect
        // position information (or no node at all).
        if let Some(node) = self.ctx.arena.get(decl_node) {
            match node.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_SPECIFIER
                | syntax_kind_ext::NAMESPACE_IMPORT
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    return;
                }
                _ => {}
            }
        } else if self.ctx.report_unresolved_imports {
            // No declaration node available — check_import_declaration will handle this
            // with correct module specifier positions from the import statement
            return;
        }

        // Don't emit TS2307 for modules in the resolved_modules set.
        // The CLI driver populates this set for modules that have been resolved
        // but whose exports might not yet be available in the binder.
        if self.module_exists_cross_file(module_specifier) {
            return;
        }

        // Don't emit for ambient module matches (declared modules, shorthand modules)
        if self.is_ambient_module_match(module_specifier) {
            return;
        }

        // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
        // IMPORTANT: Mark as emitted BEFORE calling self.error() to prevent race conditions
        // where multiple code paths check the set simultaneously
        let module_key = module_specifier.to_string();
        if self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            return; // Already emitted - skip duplicate
        }
        self.ctx.modules_with_ts2307_emitted.insert(module_key);

        // Try to find the import declaration node to get the module specifier span
        let (start, length) = if decl_node.is_some() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                // For import equals declarations, try to get the module specifier node
                if node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                    if let Some(import) = self.ctx.arena.get_import_decl(node) {
                        if let Some(module_node) = self.ctx.arena.get(import.module_specifier) {
                            // Found the module specifier node - use its span
                            (module_node.pos, module_node.end - module_node.pos)
                        } else {
                            // Fall back to the declaration node span
                            (node.pos, node.end - node.pos)
                        }
                    } else {
                        (node.pos, node.end - node.pos)
                    }
                } else if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                    // For ES6 import declarations, the module specifier should be available
                    if let Some(import) = self.ctx.arena.get_import_decl(node) {
                        if let Some(module_node) = self.ctx.arena.get(import.module_specifier) {
                            // Found the module specifier node - use its span
                            (module_node.pos, module_node.end - module_node.pos)
                        } else {
                            // Fall back to the declaration node span
                            (node.pos, node.end - node.pos)
                        }
                    } else {
                        (node.pos, node.end - node.pos)
                    }
                } else if node.kind == syntax_kind_ext::IMPORT_SPECIFIER {
                    // For import specifiers, try to find the parent import declaration
                    if let Some(ext) = self.ctx.arena.get_extended(decl_node) {
                        let parent = ext.parent;
                        if let Some(parent_node) = self.ctx.arena.get(parent) {
                            if parent_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                                if let Some(import) = self.ctx.arena.get_import_decl(parent_node) {
                                    if let Some(module_node) =
                                        self.ctx.arena.get(import.module_specifier)
                                    {
                                        // Found the module specifier node - use its span
                                        (module_node.pos, module_node.end - module_node.pos)
                                    } else {
                                        // Fall back to the parent declaration node span
                                        (parent_node.pos, parent_node.end - parent_node.pos)
                                    }
                                } else {
                                    (parent_node.pos, parent_node.end - parent_node.pos)
                                }
                            } else {
                                (node.pos, node.end - node.pos)
                            }
                        } else {
                            (node.pos, node.end - node.pos)
                        }
                    } else {
                        (node.pos, node.end - node.pos)
                    }
                } else {
                    // Use the declaration node span for other cases
                    (node.pos, node.end - node.pos)
                }
            } else {
                // No node available - use position 0
                (0, 0)
            }
        } else {
            // No declaration node - use position 0
            (0, 0)
        };

        // Note: We use self.error() which already checks emitted_diagnostics for deduplication
        // The key is (start, code), so we won't emit duplicate errors at the same location

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        // The driver's ModuleResolver may have a more specific error code than TS2307.
        if let Some(error) = self.ctx.get_resolution_error(module_specifier) {
            // For Node.js built-in modules, use TS2591 instead of TS2307.
            //
            // The resolver is the source of truth for TS2307 vs TS2792: it
            // already applies tsc's "would node-style resolution help?" check
            // before suggesting the nodenext hint. Don't re-derive TS2792 here
            // from `implied_classic_resolution` — doing so would over-trigger
            // the hint for specifiers where switching resolution modes
            // wouldn't actually resolve them (e.g., ambient-only modules).
            let (error_message, error_code) = {
                let (msg, code) = self.module_not_found_diagnostic(module_specifier);
                if code != error.code {
                    (msg, code) // module_not_found_diagnostic upgraded to TS2591
                } else {
                    (error.message.clone(), error.code)
                }
            };
            if error_code == 6504 {
                self.error_program_level(error_message, error_code);
                return;
            }
            self.error(start, length, error_message, error_code);
            return;
        }

        // Fallback: use centralized module_not_found_diagnostic which handles
        // Node.js built-in module substitution (TS2591) and Classic resolution (TS2792).
        let (message, code) = self.module_not_found_diagnostic(module_specifier);
        self.error(start, length, message, code);
    }

    /// Emit TS1192 error when a module has no default export, or TS2732 for JSON files.
    ///
    /// This is emitted when trying to use a default import (`import X from 'mod'`)
    /// but the module doesn't export a default binding.
    ///
    /// For JSON files (.json extension), emits TS2732 when `resolveJsonModule` is disabled,
    /// suggesting to enable the flag. This takes precedence over TS1192.
    ///
    /// Note: TS1192 is only suppressed when synthetic default imports are
    /// enabled for CommonJS-shaped modules. Pure ESM modules still require an
    /// explicit `default` export.
    pub(crate) fn emit_no_default_export_error(
        &mut self,
        module_specifier: &str,
        decl_node: NodeIndex,
        is_source_file_import: bool,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let mut named_default_specifier_node: Option<NodeIndex> = None;

        if let Some(node) = self.ctx.arena.get(decl_node)
            && node.kind == syntax_kind_ext::IMPORT_SPECIFIER
            && let Some(specifier) = self.ctx.arena.get_specifier(node)
        {
            let imported_name_idx = if specifier.property_name.is_none() {
                specifier.name
            } else {
                specifier.property_name
            };
            if let Some(imported_name_node) = self.ctx.arena.get(imported_name_idx)
                && let Some(imported_ident) = self.ctx.arena.get_identifier(imported_name_node)
                && imported_ident.escaped_text.as_str() == "default"
            {
                named_default_specifier_node = Some(decl_node);
            }
        }

        if named_default_specifier_node.is_none() {
            let mut current = decl_node;
            let mut import_decl_idx = NodeIndex::NONE;
            for _ in 0..8 {
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                let parent = ext.parent;
                if parent.is_none() {
                    break;
                }
                let Some(parent_node) = self.ctx.arena.get(parent) else {
                    break;
                };
                if parent_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                    import_decl_idx = parent;
                    break;
                }
                current = parent;
            }

            if import_decl_idx.is_some()
                && let Some(import_decl_node) = self.ctx.arena.get(import_decl_idx)
                && let Some(import_decl) = self.ctx.arena.get_import_decl(import_decl_node)
                && let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause)
                && let Some(clause) = self.ctx.arena.get_import_clause(clause_node)
                && let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings)
                && bindings_node.kind == syntax_kind_ext::NAMED_IMPORTS
                && let Some(named_imports) = self.ctx.arena.get_named_imports(bindings_node)
            {
                for &element_idx in &named_imports.elements.nodes {
                    let Some(element_node) = self.ctx.arena.get(element_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.ctx.arena.get_specifier(element_node) else {
                        continue;
                    };
                    let imported_name_idx = if specifier.property_name.is_none() {
                        specifier.name
                    } else {
                        specifier.property_name
                    };
                    let Some(imported_name_node) = self.ctx.arena.get(imported_name_idx) else {
                        continue;
                    };
                    let Some(imported_ident) = self.ctx.arena.get_identifier(imported_name_node)
                    else {
                        continue;
                    };
                    if imported_ident.escaped_text.as_str() == "default" {
                        named_default_specifier_node = Some(element_idx);
                        break;
                    }
                }
            }
        }

        let has_json_default_export =
            self.module_has_json_default_export(module_specifier, Some(self.ctx.current_file_idx));

        if let Some(specifier_node) = named_default_specifier_node {
            if has_json_default_export {
                return;
            }
            self.emit_no_exported_member_error(module_specifier, "default", specifier_node);
            return;
        }

        // Check if this is a JSON file import.
        // - Without resolveJsonModule: TS2732 takes precedence over TS1192.
        // - With resolveJsonModule: JSON modules always have a default export
        //   (the parsed JSON content), so TS1192 must be suppressed.
        // IMPORTANT: This check must come BEFORE report_unresolved_imports guard
        // because TS2732 should be emitted even in single-file mode.
        if has_json_default_export {
            return;
        }
        if module_specifier.ends_with(".json") && !self.ctx.compiler_options.resolve_json_module {
            // Get span from declaration node
            let (start, length) = if decl_node.is_some() {
                if let Some(node) = self.ctx.arena.get(decl_node) {
                    (node.pos, node.end - node.pos)
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            };

            use crate::diagnostics::{diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_MODULE_CONSIDER_USING_RESOLVEJSONMODULE_TO_IMPORT_MODULE_WITH_JSON_E,
                &[module_specifier],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::CANNOT_FIND_MODULE_CONSIDER_USING_RESOLVEJSONMODULE_TO_IMPORT_MODULE_WITH_JSON_E,
            );
            return;
        }

        let resolved_import_target = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, module_specifier)
            .or_else(|| self.ctx.resolve_import_target(module_specifier));

        // Only let report_unresolved_imports suppress diagnostics for truly
        // unresolved modules. A resolved module with no default export still
        // needs TS1192 even in checked-JS rechecks.
        if !self.ctx.report_unresolved_imports && resolved_import_target.is_none() {
            return;
        }

        // In `module: system`, source `.ts` files can still be default-imported
        // through the module namespace object even when
        // `allowSyntheticDefaultImports` is explicitly false.
        if is_source_file_import
            && self.ctx.compiler_options.module == tsz_common::common::ModuleKind::System
            && !self.module_has_export_equals(module_specifier)
            && !self.module_has_export_assignment_declaration(module_specifier)
        {
            return;
        }

        if self.ctx.compiler_options.module.is_node_module()
            && self.module_can_use_synthetic_default_import(module_specifier)
        {
            return;
        }

        // allowSyntheticDefaultImports suppresses TS1192 for non-source-file modules
        // (.d.ts, .js) that can use synthetic default imports. For .ts source files,
        // tsc always emits TS1192 when there is no default export — the developer
        // should add an explicit `export default`.
        //
        // When esModuleInterop is true, tsc always suppresses TS1192 for .d.ts
        // imports because the interop helper synthesizes default exports for all
        // module formats. The file_is_esm_map marks all files as ESM when the
        // compiler module is ES2015+, but this should not prevent suppression
        // when esModuleInterop explicitly enables synthetic defaults.
        //
        // When only allowSyntheticDefaultImports is true (without esModuleInterop),
        // suppression applies to CJS-shaped modules. ESM .d.ts files (from packages
        // with "type": "module") still require an explicit default export.
        let target_is_js_with_esm_syntax = resolved_import_target
            .is_some_and(|idx| self.source_file_idx_is_js_with_esm_syntax(idx));

        if self.ctx.allow_synthetic_default_imports()
            && !is_source_file_import
            && !target_is_js_with_esm_syntax
        {
            // esModuleInterop: suppress TS1192 for non-source-file imports unless
            // the module is from a genuine ESM package (e.g., node_modules with
            // package.json "type": "module"). The file_is_esm_map marks all files
            // as ESM when the compiler module is ES2015+, so module_is_esm alone
            // cannot distinguish "ESM because of package" vs "ESM because of
            // compiler mode". We additionally check if the file is in node_modules
            // to identify genuine package ESM.
            if self.ctx.compiler_options.es_module_interop {
                // Treat files with unambiguous ESM extensions (.mjs/.mts/.d.mts) as
                // genuine ESM regardless of location — they're ESM because of the
                // extension, not because the compiler module is ES2015+.
                let is_package_esm = self.module_is_esm(module_specifier)
                    && (self.module_file_is_in_node_modules(module_specifier)
                        || self.module_has_explicit_esm_extension(module_specifier));
                if !is_package_esm {
                    return;
                }
            }
            if self.module_can_use_synthetic_default_import(module_specifier) {
                return;
            }
            // For non-source-file imports (.d.ts), also suppress when the module is
            // not positively identified as ESM. Plain .d.ts files without a "type":
            // "module" package.json are assumed to be CJS-compatible.
            if !self.module_is_esm(module_specifier) {
                return;
            }
        }

        // Get span from declaration node
        let (start, length) = if decl_node.is_some() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                (node.pos, node.end - node.pos)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        use crate::diagnostics::{diagnostic_messages, format_message};

        let has_export_equals = self.module_has_export_equals(module_specifier)
            || self.module_has_export_assignment_declaration(module_specifier);

        // `export =` inside an ESM-extension module (.mts/.mjs/.d.mts) is
        // usually a syntax error (TS1203) and does not provide a default
        // export. `module: preserve` permits `export =`, so do not add TS1192
        // on consumers in that mode.
        let explicit_esm_export_equals_suppresses_default = self
            .module_has_explicit_esm_extension(module_specifier)
            && self.ctx.compiler_options.module != tsz_common::common::ModuleKind::Preserve;
        let export_equals_provides_default = has_export_equals
            && !explicit_esm_export_equals_suppresses_default
            && !target_is_js_with_esm_syntax;

        if export_equals_provides_default {
            // TS1259: "Module X can only be default-imported using the 'allowSyntheticDefaultImports' flag"
            // Only emitted for export= modules when allowSyntheticDefaultImports is false.
            if !self.ctx.allow_synthetic_default_imports() {
                let message = format_message(
                    diagnostic_messages::MODULE_CAN_ONLY_BE_DEFAULT_IMPORTED_USING_THE_FLAG,
                    &[module_specifier, "allowSyntheticDefaultImports"],
                );
                self.error(
                    start,
                    length,
                    message,
                    diagnostic_codes::MODULE_CAN_ONLY_BE_DEFAULT_IMPORTED_USING_THE_FLAG,
                );
            }
            return;
        }

        // TS1192: "Module X has no default export"
        // tsc formats the module name as the symbol name (without ./ prefix),
        // wrapped in double quotes, e.g., Module '"server"' has no default export.
        let display_name = self.imported_namespace_display_module_name(module_specifier);
        let quoted_name = format!("\"{display_name}\"");
        let message = format_message(
            diagnostic_messages::MODULE_HAS_NO_DEFAULT_EXPORT,
            &[&quoted_name],
        );
        self.error(
            start,
            length,
            message,
            diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT,
        );
    }

    pub(crate) fn module_has_json_default_export(
        &mut self,
        module_specifier: &str,
        source_file_idx: Option<usize>,
    ) -> bool {
        self.json_module_type_for_module(module_specifier, source_file_idx)
            .is_some()
    }

    pub(crate) fn module_can_use_synthetic_default_import(
        &mut self,
        module_specifier: &str,
    ) -> bool {
        // Files with unambiguous ESM extensions (.mjs/.mts/.d.mts) never provide
        // a synthetic default. An `export =` in such a file is a syntax error
        // (TS1203) and does not synthesize a default export for consumers.
        if self.module_has_explicit_esm_extension(module_specifier) {
            return false;
        }

        if self.module_has_export_equals(module_specifier)
            || self.module_has_export_assignment_declaration(module_specifier)
        {
            return true;
        }

        let target_idx = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, module_specifier)
            .or_else(|| self.ctx.resolve_import_target(module_specifier));
        if let Some(target_idx) = target_idx
            && self.source_file_idx_is_js_with_esm_syntax(target_idx)
        {
            return false;
        }
        if self
            .resolve_js_export_surface_for_module(module_specifier, Some(self.ctx.current_file_idx))
            .is_some_and(|surface| surface.has_commonjs_exports)
        {
            return true;
        }

        let Some(target_idx) = target_idx else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        let file_name = source_file.file_name.as_str();

        if file_name.ends_with(".cjs") || file_name.ends_with(".cts") {
            return true;
        }
        if file_name.ends_with(".mjs") || file_name.ends_with(".mts") {
            return false;
        }
        self.lookup_file_is_esm(file_name)
            .is_some_and(|is_esm| !is_esm)
    }

    /// Check if the target module's resolved file is in a `node_modules` directory.
    /// This helps distinguish between files that are ESM because of their package
    /// context vs files that are ESM because of the compiler's module setting.
    fn module_file_is_in_node_modules(&self, module_specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(module_specifier) else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        path_has_node_modules_segment(&source_file.file_name)
    }

    /// Check if the target module's resolved file has an unambiguously ESM
    /// extension (`.mjs`, `.mts`, or `.d.mts`). Such files are genuine ESM
    /// regardless of compiler module mode or package location, so callers can
    /// distinguish "ESM because of extension" from "ESM because of compiler
    /// module: ES2015+".
    fn module_has_explicit_esm_extension(&self, module_specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(module_specifier) else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        let name = source_file.file_name.as_str();
        name.ends_with(".mjs") || name.ends_with(".mts")
    }

    pub(crate) fn module_has_export_equals(&self, module_specifier: &str) -> bool {
        if self
            .ctx
            .binder
            .module_exports
            .get(module_specifier)
            .is_some_and(|exports| exports.has("export="))
        {
            return true;
        }

        if self
            .resolve_cross_file_namespace_exports(module_specifier)
            .is_some_and(|exports| exports.has("export="))
        {
            return true;
        }

        false
    }

    /// Resolve a named export through an `export =` target's members.
    ///
    /// This supports declaration patterns like:
    /// `declare module "m" { namespace e { interface X {} } export = e }`
    /// where `import { X } from "m"` should resolve via the export-assignment target.
    pub(crate) fn resolve_named_export_via_export_equals(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let mut visited = AliasCycleTracker::new();
        self.resolve_named_export_via_export_equals_tracked(
            module_specifier,
            export_name,
            &mut visited,
        )
    }

    /// Cycle-aware variant of [`resolve_named_export_via_export_equals`]. Shares
    /// the caller's `visited_aliases` set with [`Self::resolve_alias_symbol`]
    /// when walking an `export=` target that itself refers to an alias. Callers
    /// already inside alias resolution must use this variant so cycle tracking
    /// is preserved across the mutual recursion boundary.
    pub(crate) fn resolve_named_export_via_export_equals_tracked(
        &self,
        module_specifier: &str,
        export_name: &str,
        visited_aliases: &mut AliasCycleTracker,
    ) -> Option<tsz_binder::SymbolId> {
        let cache_key = (
            self.ctx.current_file_idx,
            module_specifier.to_string(),
            export_name.to_string(),
        );
        let cache_miss = visited_aliases.len() == 0;
        if let Some(cached) = self
            .ctx
            .export_equals_named_cache
            .borrow()
            .get(&cache_key)
            .copied()
            && (cached.is_some() || cache_miss)
        {
            return cached;
        }

        let resolved = stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || {
            self.resolve_named_export_via_export_equals_tracked_uncached(
                module_specifier,
                export_name,
                visited_aliases,
            )
        });
        if resolved.is_some() || cache_miss {
            self.ctx
                .export_equals_named_cache
                .borrow_mut()
                .insert(cache_key, resolved);
        }
        resolved
    }

    fn resolve_named_export_via_export_equals_tracked_uncached(
        &self,
        module_specifier: &str,
        export_name: &str,
        visited_aliases: &mut AliasCycleTracker,
    ) -> Option<tsz_binder::SymbolId> {
        let symbol_export_named_member =
            |symbol: &tsz_binder::Symbol, member_name: &str| -> Option<tsz_binder::SymbolId> {
                if let Some(exports) = symbol.exports.as_ref()
                    && let Some(sym_id) = exports.get(member_name)
                {
                    return Some(sym_id);
                }
                if let Some(members) = symbol.members.as_ref()
                    && let Some(sym_id) = members.get(member_name)
                {
                    return Some(sym_id);
                }
                None
            };

        let lookup_symbol = |sym_id: tsz_binder::SymbolId| -> Option<&tsz_binder::Symbol> {
            if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
                return Some(sym);
            }
            // O(1) fast-path: check resolve_symbol_file_index before O(N) binder scan
            {
                let file_idx = self.ctx.resolve_symbol_file_index(sym_id);
                if let Some(file_idx) = file_idx
                    && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
                    && let Some(sym) = binder.get_symbol(sym_id)
                {
                    return Some(sym);
                }
            }
            self.ctx
                .all_binders
                .as_ref()
                .and_then(|binders| binders.iter().find_map(|binder| binder.get_symbol(sym_id)))
        };

        let lookup_by_name = |name: &str| -> Vec<tsz_binder::SymbolId> {
            if let Some(cached) = self
                .ctx
                .symbol_name_candidates_cache
                .borrow()
                .get(name)
                .cloned()
            {
                return cached;
            }

            let mut result: Vec<tsz_binder::SymbolId> = self
                .ctx
                .binder
                .get_symbols()
                .find_all_by_name(name)
                .to_vec();
            if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                let mut seen: FxHashSet<tsz_binder::SymbolId> = result.iter().copied().collect();
                for binder in all_binders.iter() {
                    for &sym_id in binder.get_symbols().find_all_by_name(name) {
                        if seen.insert(sym_id) {
                            result.push(sym_id);
                        }
                    }
                }
            }
            self.ctx
                .symbol_name_candidates_cache
                .borrow_mut()
                .insert(name.to_string(), result.clone());
            result
        };
        let prefer_value_named_member = |member_id: tsz_binder::SymbolId| -> tsz_binder::SymbolId {
            let Some(member_symbol) = lookup_symbol(member_id) else {
                return member_id;
            };
            if (member_symbol.flags
                & (symbol_flags::CLASS
                    | symbol_flags::FUNCTION
                    | symbol_flags::VARIABLE
                    | symbol_flags::ENUM))
                != 0
            {
                return member_id;
            }
            if (member_symbol.flags
                & (symbol_flags::MODULE
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE))
                == 0
            {
                return member_id;
            }
            for candidate_id in lookup_by_name(&member_symbol.escaped_name) {
                let Some(candidate_symbol) = lookup_symbol(candidate_id) else {
                    continue;
                };
                if (candidate_symbol.flags
                    & (symbol_flags::CLASS
                        | symbol_flags::FUNCTION
                        | symbol_flags::VARIABLE
                        | symbol_flags::ENUM))
                    != 0
                {
                    return candidate_id;
                }
            }
            member_id
        };

        let resolve_from_export_equals_sym = |export_equals_sym: tsz_binder::SymbolId,
                                              visited_aliases: &mut AliasCycleTracker|
         -> Option<tsz_binder::SymbolId> {
            if export_name == "default" {
                return Some(export_equals_sym);
            }
            let mut candidate_symbol_ids = vec![export_equals_sym];

            // If `export =` points at an alias, follow the alias chain first.
            // This is required for ambient patterns like:
            //   namespace a.b { class C {} } export = a.b;
            // where named imports should resolve via members on `a.b`.
            if let Some(export_equals_symbol) = lookup_symbol(export_equals_sym)
                && export_equals_symbol.has_any_flags(symbol_flags::ALIAS)
            {
                if let Some(resolved_export_equals) =
                    self.resolve_alias_symbol(export_equals_sym, visited_aliases)
                    && resolved_export_equals != export_equals_sym
                {
                    candidate_symbol_ids.push(resolved_export_equals);
                }

                // For `export = alias` where alias is an import-equals qualified
                // name (`import x = a.b`), resolve the qualified target too.
                for decl_idx in export_equals_symbol.all_declarations() {
                    if !decl_idx.is_some() {
                        continue;
                    }
                    if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                        && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        && let Some(import_decl) = self.ctx.arena.get_import_decl(decl_node)
                    {
                        let module_ref = import_decl.module_specifier;
                        if let Some(module_ref_node) = self.ctx.arena.get(module_ref)
                            && module_ref_node.kind != SyntaxKind::StringLiteral as u16
                            && let Some(target_id) = self.resolve_qualified_symbol(module_ref)
                        {
                            candidate_symbol_ids.push(target_id);
                        }
                    }
                }
            }

            let mut seen_symbol_ids = rustc_hash::FxHashSet::default();
            for sym_id in candidate_symbol_ids {
                if !seen_symbol_ids.insert(sym_id) {
                    continue;
                }
                let Some(candidate_symbol) = lookup_symbol(sym_id) else {
                    continue;
                };

                if let Some(member_id) = symbol_export_named_member(candidate_symbol, export_name) {
                    return Some(prefer_value_named_member(member_id));
                }

                // Namespace-merge fallback (function/class + namespace split symbols).
                let merged_candidates = lookup_by_name(&candidate_symbol.escaped_name);
                for candidate_id in merged_candidates {
                    if !seen_symbol_ids.insert(candidate_id) {
                        continue;
                    }
                    let Some(merged_symbol) = lookup_symbol(candidate_id) else {
                        continue;
                    };
                    if (merged_symbol.flags
                        & (symbol_flags::MODULE
                            | symbol_flags::NAMESPACE_MODULE
                            | symbol_flags::VALUE_MODULE))
                        == 0
                    {
                        continue;
                    }
                    if let Some(member_id) = symbol_export_named_member(merged_symbol, export_name)
                    {
                        return Some(prefer_value_named_member(member_id));
                    }
                }
            }

            None
        };
        let resolve_from_exports = |exports: &tsz_binder::SymbolTable,
                                    visited_aliases: &mut AliasCycleTracker|
         -> Option<tsz_binder::SymbolId> {
            let export_equals_sym = exports.get("export=")?;
            resolve_from_export_equals_sym(export_equals_sym, visited_aliases)
        };

        let candidates = module_specifier_candidates(module_specifier);
        for candidate in &candidates {
            if let Some(exports) = self
                .ctx
                .module_exports_for_module(self.ctx.binder, candidate)
                && let Some(sym_id) = resolve_from_exports(exports, visited_aliases)
            {
                return Some(sym_id);
            }
        }

        if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            if let Some(module_binder_index) = self.ctx.global_module_binder_index.as_ref() {
                let mut checked = rustc_hash::FxHashSet::default();
                for candidate in &candidates {
                    let Some(file_indices) = module_binder_index.get(candidate) else {
                        continue;
                    };
                    for &file_idx in file_indices {
                        if checked.insert((file_idx, candidate.as_str()))
                            && let Some(binder) = all_binders.get(file_idx)
                            && let Some(exports) =
                                self.ctx.module_exports_for_module(binder, candidate)
                            && let Some(sym_id) = resolve_from_exports(exports, visited_aliases)
                        {
                            return Some(sym_id);
                        }
                    }
                }
            } else {
                for candidate in &candidates {
                    for binder in all_binders.iter() {
                        if let Some(exports) = self.ctx.module_exports_for_module(binder, candidate)
                            && let Some(sym_id) = resolve_from_exports(exports, visited_aliases)
                        {
                            return Some(sym_id);
                        }
                    }
                }
            }
        }

        if let Some(target_idx) = self.ctx.resolve_import_target(module_specifier)
            && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
        {
            if let Some(target_file_name) = self
                .ctx
                .get_arena_for_file(target_idx as u32)
                .source_files
                .first()
                .map(|sf| sf.file_name.clone())
                && let Some(exports) = self
                    .ctx
                    .module_exports_for_module(target_binder, &target_file_name)
                && let Some(sym_id) = resolve_from_exports(exports, visited_aliases)
            {
                self.ctx.register_symbol_file_target(sym_id, target_idx);
                return Some(sym_id);
            }

            if let Some(exports) = self
                .ctx
                .module_exports_for_module(target_binder, module_specifier)
                && let Some(sym_id) = resolve_from_exports(exports, visited_aliases)
            {
                self.ctx.register_symbol_file_target(sym_id, target_idx);
                return Some(sym_id);
            }
        }

        if let Some(exports) = self.resolve_cross_file_namespace_exports(module_specifier)
            && let Some(sym_id) = resolve_from_exports(&exports, visited_aliases)
        {
            return Some(sym_id);
        }

        // Global ambient-module index fallback: module_specifier -> export name ->
        // (file_idx, SymbolId). This catches declared modules that are indexed
        // globally but not directly reachable through local module_exports maps.
        if let Some(global_exports_index) = self.ctx.global_module_exports_index.as_ref() {
            for candidate in module_specifier_candidates(module_specifier) {
                if let Some(by_name) = global_exports_index.get(&candidate)
                    && let Some(entries) = by_name.get("export=")
                {
                    for &(file_idx, export_equals_sym_id) in entries {
                        self.ctx
                            .register_symbol_file_target(export_equals_sym_id, file_idx);
                        if let Some(sym_id) =
                            resolve_from_export_equals_sym(export_equals_sym_id, visited_aliases)
                        {
                            return Some(sym_id);
                        }
                    }
                }
            }
        }

        // Fallback: ambient module declarations may not always be indexed in
        // `module_exports` maps (especially in reduced/single-binder contexts).
        // Probe module-like symbols by name and resolve through their own exports.
        //
        // This probe scans every binder for symbols whose name equals the module
        // specifier, which is O(total symbols). It only ever finds `declare
        // module "name"` ambient module symbols, which are keyed by the bare
        // specifier string. When the specifier already resolves to a real source
        // file, any `export =` it carries was reached through the
        // `module_exports`/global-index paths above, and no symbol is named after
        // a relative/resolved path, so the scan cannot contribute a hit. Skipping
        // it for file-backed specifiers removes the dominant cost on large
        // module-graph projects without changing which symbol resolves.
        // Guarded by `TSZ_DISABLE_EXPORT_EQUALS_FAST_PATH` for parity checks.
        if !reexports::export_equals_fast_path_disabled()
            && self.ctx.resolve_import_target(module_specifier).is_some()
        {
            return None;
        }
        let mut ambient_module_symbol_ids = rustc_hash::FxHashSet::default();
        for candidate in module_specifier_candidates(module_specifier) {
            if let Some(sym_id) = self.ctx.binder.file_locals.get(&candidate) {
                ambient_module_symbol_ids.insert(sym_id);
            }
            for &sym_id in self.ctx.binder.get_symbols().find_all_by_name(&candidate) {
                ambient_module_symbol_ids.insert(sym_id);
            }
            if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                for binder in all_binders.iter() {
                    if let Some(sym_id) = binder.file_locals.get(&candidate) {
                        ambient_module_symbol_ids.insert(sym_id);
                    }
                    for &sym_id in binder.get_symbols().find_all_by_name(&candidate) {
                        ambient_module_symbol_ids.insert(sym_id);
                    }
                }
            }
        }
        for module_sym_id in ambient_module_symbol_ids {
            let Some(module_symbol) = lookup_symbol(module_sym_id) else {
                continue;
            };
            if (module_symbol.flags
                & (symbol_flags::MODULE
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE))
                == 0
            {
                continue;
            }
            if let Some(exports) = module_symbol.exports.as_ref()
                && let Some(sym_id) = resolve_from_exports(exports, visited_aliases)
            {
                return Some(sym_id);
            }
        }
        None
    }

    /// Emit TS2305 error when a module has no exported member with the given name.
    ///
    /// This is emitted when trying to use a named import (`import { X } from 'mod'`)
    /// but the module doesn't export a member named 'X'.
    pub(crate) fn emit_no_exported_member_error(
        &mut self,
        module_specifier: &str,
        member_name: &str,
        decl_node: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Only emit if report_unresolved_imports is enabled
        if !self.ctx.report_unresolved_imports {
            return;
        }

        // Get span from declaration node
        let (start, length) = if decl_node.is_some() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                (node.pos, node.end - node.pos)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        let has_default =
            if let Some(exports_table) = self.resolve_effective_module_exports(module_specifier) {
                exports_table.has("default")
                    || exports_table.has("export=")
                    || self.module_uses_module_exports_interop(
                        module_specifier,
                        Some(self.current_file_emit_resolution_mode()),
                    )
            } else {
                false
            };

        use crate::diagnostics::{diagnostic_messages, format_message};
        // TSC includes source-level quotes in module diagnostic messages
        let quoted_module = format!("\"{module_specifier}\"");
        let suggestion = self
            .resolve_effective_module_exports(module_specifier)
            .and_then(|exports| {
                let export_names: Vec<&str> =
                    exports.iter().map(|(name, _)| name.as_str()).collect();
                tsz_parser::parser::spelling::get_spelling_suggestion(member_name, &export_names)
                    .map(|s| s.to_string())
            });

        if let Some(suggestion) = suggestion {
            let message = format_message(
                diagnostic_messages::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
                &[&quoted_module, member_name, &suggestion],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
            );
        } else if has_default && member_name != "default" {
            let message = format_message(
                diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                &[&quoted_module, member_name],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
            );
        } else {
            let message = format_message(
                diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                &[&quoted_module, member_name],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
            );
        }
    }

    /// Check if a module exists for cross-file resolution.
    ///
    /// Returns true if the module can be found via `resolved_modules`, through
    /// the context's cross-file resolution mechanism, or via global binder indices.
    pub(crate) fn module_exists_cross_file(&self, module_name: &str) -> bool {
        if self.ctx.resolve_import_target(module_name).is_some() {
            return true;
        }

        // Check if it's in resolved_modules (set by the driver for multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return true;
        }

        // O(1) check via global_module_binder_index: any binder with module_exports
        // for this specifier means the module exists as an ambient declaration.
        if self.ctx.files_for_module_specifier(module_name).is_some() {
            return true;
        }

        // O(1) check via global_declared_modules: covers `declare module "X"` and
        // shorthand ambient modules across all files.
        if let Some(declared) = &self.ctx.global_declared_modules {
            let normalized = module_name.trim().trim_matches('"').trim_matches('\'');
            if declared.exact.contains(normalized) {
                return true;
            }
            // Small linear scan over wildcard patterns only
            for pattern in &declared.patterns {
                let p = pattern.trim().trim_matches('"').trim_matches('\'');
                if let Some(prefix) = p.strip_suffix('*')
                    && normalized.starts_with(prefix)
                {
                    return true;
                }
            }
        }

        false
    }
}
