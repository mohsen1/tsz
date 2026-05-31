//! Main import declaration check (`check_import_declaration`).
//!
//! Contains the top-level import statement validation entry point that orchestrates
//! module resolution, import attribute checks, extension diagnostics (TS5097, TS2876,
//! TS2877), CJS/ESM boundary checks (TS1479, TS1541), and import member validation.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;

use super::declaration_helpers::{imported_types_package_target, is_node_builtin_module};
use super::declaration_helpers::{should_rewrite_module_specifier, ts_extension_suffix};

impl<'a> CheckerState<'a> {
    /// Check an import declaration for unresolved modules and missing exports.
    pub(crate) fn check_import_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };
        let request_kind = crate::context::ResolutionRequestKind::EsmImport;
        let request_resolution_mode = self.ctx.resolution_mode_for_request(
            request_kind,
            self.get_resolution_mode_override(import.attributes),
        );

        let is_type_only_import = self
            .ctx
            .arena
            .get(import.import_clause)
            .and_then(|clause_node| self.ctx.arena.get_import_clause(clause_node))
            .is_some_and(|clause| clause.is_type_only);

        // Suppress semantic diagnostics (TS2307, TS2823, TS2322) when the import
        // statement has parse errors. A wrong module-element context is a grammar
        // diagnostic, not a reason to skip module/member validation: tsc still
        // reports missing modules and missing named exports for imports in a bare
        // block after TS1232.
        let in_wrong_context = self.is_in_non_module_element_context(stmt_idx);
        let wrong_context_allows_module_semantics = in_wrong_context
            && !self.is_inside_function_body(stmt_idx)
            && !self.is_inside_namespace_declaration(stmt_idx);
        let has_parse_errors = node.this_or_subtree_has_error()
            || (self.ctx.has_real_syntax_errors && !wrong_context_allows_module_semantics);
        if in_wrong_context && self.is_inside_function_body(stmt_idx) {
            return;
        }

        // TS18058/TS18059: Validate deferred import binding restrictions.
        // Deferred imports only allow namespace imports: `import defer * as ns from "..."`
        self.check_deferred_import_restrictions(import.import_clause);

        // TS1363: A type-only import can specify a default import or named bindings, but not both.
        // e.g., `import type A, { B } from '...'` is invalid.
        if let Some(clause_node) = self.ctx.arena.get(import.import_clause)
            && let Some(clause) = self.ctx.arena.get_import_clause(clause_node)
            && clause.is_type_only
            && clause.name.is_some()
            && clause.named_bindings.is_some()
        {
            self.error_at_node(
                        import.import_clause,
                        "A type-only import can specify a default import or named bindings, but not both.",
                        diagnostic_codes::A_TYPE_ONLY_IMPORT_CAN_SPECIFY_A_DEFAULT_IMPORT_OR_NAMED_BINDINGS_BUT_NOT_BOTH,
                    );
        }

        // TS2880: Warn about deprecated `assert` keyword
        self.check_import_attributes_deprecated_assert(import.attributes);

        if !has_parse_errors {
            self.check_type_only_resolution_mode_attribute_grammar(
                import.attributes,
                is_type_only_import,
            );

            // TS2823: Import attributes require specific module options
            self.check_import_attributes_module_option(import.attributes, is_type_only_import);

            // TS2322: Check import attribute values against global ImportAttributes interface
            self.check_import_attributes_assignability(import.attributes);

            self.check_import_attributes_commonjs_or_type_only(
                import.attributes,
                is_type_only_import,
            );
        }

        // TS1214/TS1212: Check import binding names for strict mode reserved words.
        // Import declarations make the file a module, so it's always strict mode → TS1214.
        self.check_import_binding_reserved_words(import.import_clause);

        if import.import_clause.is_some() {
            self.check_import_declaration_conflicts(stmt_idx, import.import_clause);
        }

        // Skip semantic import diagnostics when the import has parse errors.
        if has_parse_errors {
            return;
        }

        // Extract module specifier data eagerly so direct import diagnostics like
        // TS6137 can run even when unresolved-import reporting is disabled.
        let module_specifier_idx = import.module_specifier;
        let import_clause_idx = import.import_clause;

        let Some(spec_node) = self.ctx.arena.get(module_specifier_idx) else {
            return;
        };
        let spec_start = spec_node.pos;
        let spec_length = spec_node.end.saturating_sub(spec_node.pos);

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;
        // tsc emits TS2307 independently per import declaration, even when multiple
        // imports reference the same module.  Clear the per-module dedup entry so
        // this declaration gets its own chance to report a module-not-found error.
        // The within-declaration dedup (resolution-error path vs fallback path)
        // is preserved because both paths insert the key before returning.
        self.ctx
            .modules_with_ts2307_emitted
            .remove(module_name.as_str());
        let has_import_clause = self.ctx.arena.get(import_clause_idx).is_some();
        let is_side_effect_import = !has_import_clause;
        // Note: side-effect imports may return early in the resolution error check below
        // when no_unchecked_side_effect_imports=false (silently ignoring unresolved modules).
        if !is_type_only_import && let Some(suggested) = imported_types_package_target(module_name)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::CANNOT_IMPORT_TYPE_DECLARATION_FILES_CONSIDER_IMPORTING_INSTEAD_OF,
                &[&suggested, module_name],
            );
            self.error_at_position(
                spec_start,
                spec_length,
                &message,
                diagnostic_codes::CANNOT_IMPORT_TYPE_DECLARATION_FILES_CONSIDER_IMPORTING_INSTEAD_OF,
            );
            return;
        }

        // Most import semantics still need to run for already-resolved modules even when
        // unresolved-import reporting is disabled (the lightweight multi-file harness
        // uses this mode). Only skip entirely when the module also can't be resolved.
        if !self.ctx.report_unresolved_imports {
            let resolution_mode = request_resolution_mode;
            self.check_js_type_only_imports_after_import_validation(import, module_name);
            let module_resolves = self
                .ctx
                .resolve_import_target_from_file_with_mode(
                    self.ctx.current_file_idx,
                    module_name,
                    resolution_mode,
                )
                .or_else(|| {
                    self.ctx
                        .resolve_import_target_from_file(self.ctx.current_file_idx, module_name)
                })
                .or_else(|| self.ctx.resolve_import_target(module_name))
                .is_some()
                || self
                    .ctx
                    .module_exports_contains_module(self.ctx.binder, module_name);
            if !module_resolves {
                return;
            }
        }
        // Side-effect imports (bare `import "module"`) are silently ignored when
        // noUncheckedSideEffectImports is disabled (the default). tsc suppresses
        // ALL resolution failures for these imports regardless of the error code.
        // Check early to avoid any error emission path below.
        if is_side_effect_import && !self.ctx.compiler_options.no_unchecked_side_effect_imports {
            return;
        }
        // Track whether TS2846/TS5097 extension diagnostics were emitted.
        // When these fire, TS2307 from module resolution should be suppressed
        // (tsc prioritizes extension-specific diagnostics over "cannot find module").
        let mut emitted_extension_diagnostic = false;

        let dts_ext = if module_name.ends_with(".d.ts") {
            Some((".d.ts", ".ts", ".js"))
        } else if module_name.ends_with(".d.mts") {
            Some((".d.mts", ".mts", ".mjs"))
        } else if module_name.ends_with(".d.cts") {
            Some((".d.cts", ".cts", ".cjs"))
        } else {
            None
        };
        // tsc only emits TS2846 when the .d.ts module actually resolves; if
        // the file doesn't exist, TS2307 (cannot find module) takes priority.
        // Without this guard we emit both TS5097/TS2846 AND tsc's TS2307,
        // producing extra diagnostics on missing imports.
        let module_resolves_dts = self
            .ctx
            .resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                request_resolution_mode,
            )
            .is_some()
            || self
                .ctx
                .module_exports_contains_module(self.ctx.binder, module_name);
        if let Some((dts_suffix, ts_ext, js_ext)) = dts_ext
            && !is_type_only_import
            && module_resolves_dts
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let base = module_name.trim_end_matches(dts_suffix);
            let suggested = if self.ctx.compiler_options.allow_importing_ts_extensions {
                format!("{base}{ts_ext}")
            } else {
                // For CommonJS-like module kinds, extensionless imports are valid.
                // For ESM-like module kinds, append .js/.mjs/.cjs extension.
                use tsz_common::common::ModuleKind;
                match self.ctx.compiler_options.module {
                    ModuleKind::CommonJS
                    | ModuleKind::AMD
                    | ModuleKind::UMD
                    | ModuleKind::System
                    | ModuleKind::None => base.to_string(),
                    _ => format!("{base}{js_ext}"),
                }
            };
            let message = format_message(
                diagnostic_messages::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
                &[&suggested],
            );
            self.error_at_position(
                spec_start,
                spec_length,
                &message,
                diagnostic_codes::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
            );
            emitted_extension_diagnostic = true;
        }

        // TS5097: Check for .ts/.tsx/.mts/.cts extensions when allowImportingTsExtensions is disabled.
        // rewriteRelativeImportExtensions also suppresses this error (tsc utilities.ts:9045).
        // tsc does not emit TS5097 inside declaration files (.d.ts).
        // When the resolver reports TS6142 (jsx not set), tsc does not also emit TS5097.
        let has_jsx_not_set_error = self
            .ctx
            .get_resolution_error_for_request(module_name, request_resolution_mode, request_kind)
            .is_some_and(|e| {
                e.code
                    == crate::diagnostics::diagnostic_codes::MODULE_WAS_RESOLVED_TO_BUT_JSX_IS_NOT_SET
            });
        // tsc only emits TS5097 when the module actually resolves (so the .ts
        // extension is the user's mistake on a real file). When the module
        // doesn't resolve at all, tsc emits TS2307 ('cannot find module')
        // instead — emitting both produces a misleading double-diagnostic.
        if !self.ctx.compiler_options.allow_importing_ts_extensions
            && !self.ctx.compiler_options.rewrite_relative_import_extensions
            && !is_type_only_import
            && !self.ctx.is_declaration_file()
            && !has_jsx_not_set_error
            && self.module_target_is_typescript_input_file(module_name)
            && let Some(ext) = ts_extension_suffix(module_name)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                    diagnostic_messages::AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS,
                    &[ext],
                );
            self.error_at_position(
                    spec_start,
                    spec_length,
                    &message,
                    diagnostic_codes::AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS,
                );
            emitted_extension_diagnostic = true;
        }

        // TS2876: rewriteRelativeImportExtensions — specifier looks like a file name
        // (e.g. `./foo.ts`) but actually resolves to a directory index file
        // (e.g. `./foo.ts/index.ts`), making extension rewriting unsafe.
        // tsc checks `!resolvedModule.resolvedUsingTsExtension && shouldRewrite`.
        if !emitted_extension_diagnostic
            && self.ctx.compiler_options.rewrite_relative_import_extensions
            && !is_type_only_import
            && !self.ctx.is_declaration_file()
            && should_rewrite_module_specifier(module_name)
            && self.resolved_via_directory_index(module_name)
        {
            let resolved_display = self.resolved_file_display_path(module_name);
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::THIS_RELATIVE_IMPORT_PATH_IS_UNSAFE_TO_REWRITE_BECAUSE_IT_LOOKS_LIKE_A_FILE_NAME,
                &[&resolved_display],
            );
            self.error_at_position(
                spec_start,
                spec_length,
                &message,
                diagnostic_codes::THIS_RELATIVE_IMPORT_PATH_IS_UNSAFE_TO_REWRITE_BECAUSE_IT_LOOKS_LIKE_A_FILE_NAME,
            );
            emitted_extension_diagnostic = true;
        }

        // TS2877: rewriteRelativeImportExtensions — non-relative imports with
        // a TypeScript extension that resolve to an input TypeScript file are not
        // rewritten during emit.
        //
        // Suppress when the resolver consumed the `.ts` via a literal
        // package.json `exports`/`imports` key (e.g. `"./*.ts": "./*.js"` or
        // `"#foo.ts": ...`). In those cases the package author has explicitly
        // opted into the `.ts`→`.js` mapping at runtime, so the import will
        // resolve correctly without rewriting. This mirrors tsc's
        // `resolvedUsingTsExtension` gate.
        if !emitted_extension_diagnostic
            && self.ctx.compiler_options.rewrite_relative_import_extensions
            && !is_type_only_import
            && !self.ctx.is_declaration_file()
            && !should_rewrite_module_specifier(module_name)
            && !self.resolved_via_directory_index(module_name)
            && self.module_target_is_typescript_input_file(module_name)
            && !self.resolved_module_is_from_node_modules(module_name)
            && !self.ctx.import_resolved_using_ts_extension(module_name)
            && let Some(ext) = ts_extension_suffix(module_name)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::THIS_IMPORT_USES_A_EXTENSION_TO_RESOLVE_TO_AN_INPUT_TYPESCRIPT_FILE_BUT_WILL_NOT,
                &[ext],
            );
            self.error_at_position(
                spec_start,
                spec_length,
                &message,
                diagnostic_codes::THIS_IMPORT_USES_A_EXTENSION_TO_RESOLVE_TO_AN_INPUT_TYPESCRIPT_FILE_BUT_WILL_NOT,
            );
            emitted_extension_diagnostic = true;
        }

        if self.would_create_cycle(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: cycle detected");
            let cycle_path: Vec<&str> = self
                .ctx
                .import_resolution_stack
                .iter()
                .map(std::string::String::as_str)
                .chain(std::iter::once(module_name.as_str()))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!("Circular import detected: {cycle_str}");

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_position(
                    spec_start,
                    spec_length,
                    &message,
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
            return;
        }

        self.ctx.import_resolution_stack.push(module_name.clone());

        // Node.js built-in modules (e.g. "fs", "path", "node:fs") should not
        // trigger TS2307/TS2882 when using Node module resolution. TSC resolves
        // these via @types/node; our single-file checker lacks this, so we
        // suppress resolution errors for known built-in names.
        let is_node_builtin = self.ctx.compiler_options.module.is_node_module()
            && is_node_builtin_module(module_name);

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        // This must be checked before resolved_modules to catch extensionless import errors
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error_for_request(
            module_name,
            request_resolution_mode,
            request_kind,
        ) {
            // Extract error values before mutable borrow
            let mut error_code = error.code;
            let mut error_message = error.message.clone();
            if error_code
                == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                || error_code == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
            {
                // When TS2846 or TS5097 was already emitted for this import,
                // suppress TS2307/TS2792. tsc prioritizes extension-specific
                // diagnostics over "cannot find module" errors.
                // Also suppress TS2307 for .d.ts type-only imports — tsc does
                // not validate module existence for `import type` from .d.ts.
                if emitted_extension_diagnostic || (is_type_only_import && dts_ext.is_some()) {
                    self.ctx.import_resolution_stack.pop();
                    return;
                }
                // A resolved triple-slash type reference can introduce ambient
                // wildcard modules for non-TS assets (for example Vite's
                // `vite/client` declarations). Those declarations should
                // suppress the resolver's missing-file diagnostic.
                if self.wildcard_ambient_module_declared(module_name) {
                    self.check_imported_members(import, module_name);
                    self.ctx.import_resolution_stack.pop();
                    return;
                }
                // Node.js built-in modules: suppress TS2307/TS2882 entirely,
                // UNLESS noTypesAndSymbols is set — in that case @types/node
                // won't be auto-loaded, so tsc emits TS2591 instead.
                if is_node_builtin {
                    if self.ctx.compiler_options.no_types_and_symbols {
                        let (msg, code) = self.module_not_found_diagnostic_for_site(
                            module_name,
                            crate::import::core::ModuleNotFoundSite::Import,
                        );
                        if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                            self.ctx
                                .modules_with_ts2307_emitted
                                .insert(module_key);
                            self.error_at_position(spec_start, spec_length, &msg, code);
                        }
                    }
                    self.ctx.import_resolution_stack.pop();
                    return;
                }

                // AMD/System/classic-resolution: tsc only emits the secondary
                // missing-module diagnostic when TS5107 deprecation is silenced
                // via `ignoreDeprecations` — issue #3077.
                if self.deprecated_mode_suppresses_module_not_found() {
                    self.ctx.import_resolution_stack.pop();
                    return;
                }

                // Side-effect imports use TS2882 instead of TS2307/TS2792,
                // but only when noUncheckedSideEffectImports is enabled.
                // When disabled (default), tsc silently ignores all resolution failures.
                if is_side_effect_import {
                    if !self.ctx.compiler_options.no_unchecked_side_effect_imports {
                        self.ctx.import_resolution_stack.pop();
                        return;
                    }
                    // noUncheckedSideEffectImports is enabled — convert to TS2882
                    if error_code
                        == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                        || error_code
                            == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        error_code = diagnostic_codes::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF;
                        error_message = format_message(
                            diagnostic_messages::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
                            &[module_name],
                        );
                    }
                } else {
                    let (fallback_message, fallback_code) = self.module_not_found_diagnostic(module_name);
                    error_code = fallback_code;
                    error_message = fallback_message;
                }
            }
            tracing::trace!(%module_name, error_code, "check_import_declaration: resolution error found");
            if error_code == 6504 {
                self.error_program_level(error_message, error_code);
                self.ctx.import_resolution_stack.pop();
                return;
            }
            // Side-effect imports: suppress ALL resolution errors when
            // noUncheckedSideEffectImports is disabled (the default).
            // The check inside the CANNOT_FIND_MODULE block above handles
            // TS2307/TS2792, but other error codes (e.g., TS2882 from the
            // conformance runner) can bypass that path. This catch-all
            // ensures no resolution error leaks for bare `import "module"`.
            if is_side_effect_import && !self.ctx.compiler_options.no_unchecked_side_effect_imports
            {
                self.ctx.import_resolution_stack.pop();
                return;
            }
            // Check if we've already emitted an error for this module (prevents duplicate emissions)
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx
                    .modules_with_ts2307_emitted
                    .insert(module_key.clone());
                self.error_at_position(spec_start, spec_length, &error_message, error_code);
            }
            if error_code
                != crate::diagnostics::diagnostic_codes::MODULE_WAS_RESOLVED_TO_BUT_JSX_IS_NOT_SET
                && error_code
                    != crate::diagnostics::diagnostic_codes::MODULE_WAS_RESOLVED_TO_BUT_ALLOWARBITRARYEXTENSIONS_IS_NOT_SET
            {
                self.ctx.import_resolution_stack.pop();
                return;
            }
        }

        // Ambient module declarations still suppress TS2307 when the driver
        // did not report a concrete resolution failure for this import.
        if self.is_ambient_module_match(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: ambient module match, returning");
            // Keep JS-mode type-only import diagnostics (TS18042) for ambient modules.
            self.check_imported_members(import, module_name);
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Use global declared modules index for O(1) lookup
        {
            let found = if let Some(declared) = &self.ctx.global_declared_modules {
                let normalized = module_name.trim_matches('"').trim_matches('\'');
                declared.exact.contains(normalized)
            } else if let Some(binders) = &self.ctx.all_binders {
                binders.iter().any(|binder| {
                    binder.declared_modules.contains(module_name)
                        || binder.shorthand_ambient_modules.contains(module_name)
                })
            } else {
                false
            };
            if found {
                tracing::trace!(%module_name, "check_import_declaration: found in declared/shorthand modules, returning");
                // Keep JS-mode type-only import diagnostics (TS18042) for ambient modules.
                self.check_imported_members(import, module_name);
                self.ctx.import_resolution_stack.pop();
                return;
            }
        }

        // For side-effect imports (import "module") in default mode (no_unchecked_side_effect_imports=false),
        // we only check resolution errors (TS2882 above). Skip member/export validation which
        // requires import bindings. If we reach here, the module resolved successfully.
        if is_side_effect_import && !self.ctx.compiler_options.no_unchecked_side_effect_imports {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Check if module was successfully resolved
        if self.resolved_module_set_contains_specifier(module_name) {
            if let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file_for_request(
                    self.ctx.current_file_idx,
                    module_name,
                    request_resolution_mode,
                    request_kind,
                )
                .or_else(|| self.ctx.resolve_import_target(module_name))
            {
                let has_typed_export_surface = self
                    .resolve_effective_module_exports_with_mode(
                        module_name,
                        self.requested_resolution_mode(import.attributes, is_type_only_import),
                    )
                    .is_some();
                // When a module was successfully resolved to a target file, do NOT
                // emit TS2307 regardless of its export surface. TS2307 means the
                // module file cannot be found at all. A file with no exports (e.g.,
                // `export {}`, `declare global`, or only side-effect imports) is
                // still a valid module — tsc never emits TS2307 for it. Specific
                // import errors (TS2305, TS2459) will be caught later during
                // member validation.
                let mut skip_export_checks = false;
                // Extract data we need before any mutable borrows
                let (_target_is_declaration_file, file_info) = {
                    let arena = self.ctx.get_arena_for_file(target_idx as u32);
                    if let Some(source_file) = arena.source_files.first() {
                        let file_name = source_file.file_name.as_str();
                        let is_js_like = file_name.ends_with(".js")
                            || file_name.ends_with(".jsx")
                            || file_name.ends_with(".mjs")
                            || file_name.ends_with(".cjs");
                        let skip_exports = is_js_like
                            && !source_file.is_declaration_file
                            && !has_typed_export_surface;
                        // Determine if target file is ESM. .mjs/.mts are always ESM.
                        // For .js/.ts targets, also check package.json "type" field via
                        // file_is_esm_map. TSC does not emit TS1479 when a .js source file
                        // imports a .js ESM target — only when the target is .mjs/.mts
                        // (unambiguously ESM). However, .cjs files are unambiguously CJS,
                        // so they DO get TS1479 when importing .js ESM targets.
                        // JSON files are data, not modules — they can always be
                        // require()'d and never count as ESM for TS1479.
                        let target_is_json = file_name.ends_with(".json");
                        let target_ext_is_esm = !target_is_json
                            && (file_name.ends_with(".mjs") || file_name.ends_with(".mts"));
                        // Skip file_is_esm_map check only for ambiguous JS sources (.js/.jsx).
                        // .cjs is unambiguously CJS, so it should check file_is_esm_map
                        // to detect .js targets that are ESM via package.json "type".
                        let skip_esm_map = target_is_json
                            || self.ctx.file_name.ends_with(".js")
                            || self.ctx.file_name.ends_with(".jsx")
                            || self.ctx.file_name.ends_with(".mjs");
                        let target_is_esm = target_ext_is_esm
                            || (!skip_esm_map
                                && self.lookup_file_is_esm(file_name).unwrap_or(false));
                        let is_dts = source_file.is_declaration_file;
                        (is_dts, Some((skip_exports, target_is_esm)))
                    } else {
                        (false, None)
                    }
                };

                if let Some((should_skip_exports, target_is_esm)) = file_info {
                    if should_skip_exports {
                        skip_export_checks = true;
                    }

                    // TS1479: Check if CommonJS file is importing an ES module.
                    // In TypeScript 6.0+, TSC only emits TS1479 for Node16/Node18
                    // module kinds. Node20 and NodeNext (targeting Node 22+) support
                    // `require()` of ESM modules, so the diagnostic is suppressed.
                    // For ESNext, Preserve, bundler, and other module kinds, the
                    // import interop is handled by the bundler/runtime.
                    let is_node_module_kind =
                        self.ctx.compiler_options.module.is_node16_or_node18();
                    let current_is_commonjs = is_node_module_kind && {
                        let current_file = &self.ctx.file_name;
                        // .cts/.cjs are always CommonJS
                        let is_commonjs_file =
                            current_file.ends_with(".cts") || current_file.ends_with(".cjs");
                        // .mts/.mjs are always ESM
                        let is_esm_file =
                            current_file.ends_with(".mts") || current_file.ends_with(".mjs");
                        if is_commonjs_file {
                            true
                        } else if is_esm_file {
                            false
                        } else if let Some(is_esm) = self.ctx.file_is_esm {
                            // Driver-provided per-file module kind from package.json
                            // "type" field (Node16/NodeNext resolution)
                            !is_esm
                        } else {
                            // Fallback: global module kind heuristic
                            !self.ctx.compiler_options.module.is_es_module()
                        }
                    };

                    // TSC suppresses TS1479 for .cjs relative imports, but .cts
                    // files still report the CJS -> ESM boundary for relative
                    // imports that resolve to ESM targets.
                    let is_explicit_cjs_js_file = self.ctx.file_name.ends_with(".cjs");
                    let is_relative_import =
                        module_name.starts_with("./") || module_name.starts_with("../");
                    let suppress_for_cjs_relative = is_relative_import && is_explicit_cjs_js_file;

                    // TS1479 only applies under Node16/Node18 module kinds where
                    // CJS/ESM interop boundaries exist at runtime. Node20/NodeNext,
                    // bundler resolution, and pure ESM module kinds handle interop
                    // transparently.
                    let module_has_cjs_esm_boundary =
                        self.ctx.compiler_options.module.is_node16_or_node18();

                    if current_is_commonjs
                        && target_is_esm
                        && module_has_cjs_esm_boundary
                        && !is_type_only_import
                        && !suppress_for_cjs_relative
                    {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let message = format_message(
                            diagnostic_messages::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_H,
                            &[module_name],
                        );
                        self.error_at_position(
                            spec_start,
                            spec_length,
                            &message,
                            diagnostic_codes::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_H,
                        );
                    }

                    // TS1541: type-only imports that cross a Node16/Node18
                    // CJS -> ESM boundary need an explicit resolution-mode.
                    if is_type_only_import
                        && self.type_only_cjs_esm_resolution_mode_is_missing(
                            target_idx,
                            self.get_resolution_mode_override(import.attributes)
                                .is_some(),
                        )
                    {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_position(
                            spec_start,
                            spec_length,
                            diagnostic_messages::TYPE_ONLY_IMPORT_OF_AN_ECMASCRIPT_MODULE_FROM_A_COMMONJS_MODULE_MUST_HAVE_A_RESO,
                            diagnostic_codes::TYPE_ONLY_IMPORT_OF_AN_ECMASCRIPT_MODULE_FROM_A_COMMONJS_MODULE_MUST_HAVE_A_RESO,
                        );
                    }
                }

                self.maybe_emit_json_esm_import_attribute_required(
                    import,
                    target_idx,
                    spec_start,
                    spec_length,
                    is_type_only_import,
                );

                // TS2846 for resolved .d.ts files is only emitted when the import
                // specifier explicitly uses a .d.ts extension (handled above at the
                // dts_ext check). TSC does NOT emit TS2846 when an import like
                // "./foo" resolves to "foo.d.ts" — even under verbatimModuleSyntax.
                self.maybe_emit_imported_global_augmentation_errors(target_idx);
                if let Some(binder) = self.ctx.get_binder_for_file(target_idx) {
                    let normalized_module_name = module_name.trim_matches('"').trim_matches('\'');
                    // Side-effect imports (`import "x"`) never require the target
                    // to be a module — they just execute the file.  Skip TS2306
                    // regardless of the noUncheckedSideEffectImports setting.
                    let arena = self.ctx.get_arena_for_file(target_idx as u32);
                    let source_file = arena.source_files.first();
                    let target_is_global_augmentation_dts =
                        source_file.is_some_and(|source_file| {
                            source_file.file_name.ends_with(".d.ts")
                                && !self
                                    .source_file_has_syntactic_module_indicator(arena, source_file)
                                && self.source_file_has_top_level_global_augmentation(
                                    arena,
                                    source_file,
                                )
                        });
                    if !is_side_effect_import
                        && (!binder.is_external_module || target_is_global_augmentation_dts)
                        && !self.is_ambient_module_match(module_name)
                        && !self
                            .ctx
                            .declared_modules_contains(binder, normalized_module_name)
                        && let Some(source_file) = source_file
                    {
                        let file_name = source_file.file_name.as_str();
                        let is_js_like = file_name.ends_with(".js")
                            || file_name.ends_with(".jsx")
                            || file_name.ends_with(".mjs")
                            || file_name.ends_with(".cjs");
                        let is_json_module = file_name.ends_with(".json")
                            && (self.ctx.compiler_options.resolve_json_module
                                || self.import_attributes_enable_json_module(import.attributes));
                        if !is_js_like && !is_json_module {
                            use crate::diagnostics::{
                                diagnostic_codes, diagnostic_messages, format_message,
                            };
                            let message = format_message(
                                diagnostic_messages::FILE_IS_NOT_A_MODULE,
                                &[&source_file.file_name],
                            );
                            self.error_at_position(
                                spec_start,
                                spec_length,
                                &message,
                                diagnostic_codes::FILE_IS_NOT_A_MODULE,
                            );
                            self.ctx.import_resolution_stack.pop();
                            return;
                        }
                    }
                }
                if !skip_export_checks {
                    self.check_imported_members(import, module_name);
                }
                self.check_js_type_only_imports_after_import_validation(import, module_name);
            } else {
                self.check_imported_members(import, module_name);
                self.check_js_type_only_imports_after_import_validation(import, module_name);
            }

            // TS1484/TS1485: verbatimModuleSyntax import checks
            self.check_verbatim_module_syntax_imports(import, module_name);

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        if self
            .ctx
            .module_exports_contains_module(self.ctx.binder, module_name)
            && self.ctx.get_resolution_error(module_name).is_none()
        {
            tracing::trace!(%module_name, "check_import_declaration: found in module_exports, checking members");
            self.check_imported_members(import, module_name);
            self.check_js_type_only_imports_after_import_validation(import, module_name);

            // TS1484/TS1485: verbatimModuleSyntax import checks
            self.check_verbatim_module_syntax_imports(import, module_name);

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Node.js built-in modules: suppress fallback TS2307/TS2882 too,
        // unless noTypesAndSymbols — emit TS2591 in that case.
        if is_node_builtin {
            if self.ctx.compiler_options.no_types_and_symbols {
                let (msg, code) = self.module_not_found_diagnostic_for_site(
                    module_name,
                    crate::import::core::ModuleNotFoundSite::Import,
                );
                if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                    self.ctx.modules_with_ts2307_emitted.insert(module_key);
                    self.error_at_position(spec_start, spec_length, &msg, code);
                }
            }
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // AMD/System/classic-resolution: same suppression rule as the
        // resolution-error branch above (issue #3077).
        if self.deprecated_mode_suppresses_module_not_found() {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        tracing::trace!(%module_name, "check_import_declaration: fallback - emitting module-not-found error");

        // Side-effect imports are silently ignored when noUncheckedSideEffectImports is false
        if is_side_effect_import && !self.ctx.compiler_options.no_unchecked_side_effect_imports {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Fallback: Emit module-not-found error if no specific error was found
        // Check if we've already emitted for this module (prevents duplicate emissions)
        if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            self.ctx.modules_with_ts2307_emitted.insert(module_key);
            // Side-effect imports (bare `import "module"`) use TS2882 instead of TS2307
            let (message, code) = if is_side_effect_import {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                (
                    format_message(
                        diagnostic_messages::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
                        &[module_name],
                    ),
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
                )
            } else {
                self.module_not_found_diagnostic(module_name)
            };
            // Use pre-extracted position instead of error_at_node to avoid
            // silent failures when get_node_span returns None
            self.error_at_position(spec_start, spec_length, &message, code);
        }

        self.ctx.import_resolution_stack.pop();
    }
}
