//! Import declaration validation (`import { X } from "y"`), re-export chain
//! cycle detection, and import resolution helpers.
//!
//! Import-equals validation (`import X = require("y")` / `import X = Namespace`)
//! lives in the sibling `import_equals_checker` module.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;

/// Returns the TypeScript extension suffix (e.g. `".ts"`, `".tsx"`) if the module path
/// ends with a TS-specific extension that requires `allowImportingTsExtensions`.
/// Returns `None` for `.d.ts`/`.d.mts`/`.d.cts` (handled separately by TS2846) and
/// non-TS extensions.
pub(super) fn ts_extension_suffix(module_name: &str) -> Option<&'static str> {
    // .d.ts/.d.mts/.d.cts are declaration files — handled by TS2846, not TS5097
    if module_name.ends_with(".d.ts")
        || module_name.ends_with(".d.mts")
        || module_name.ends_with(".d.cts")
    {
        return None;
    }
    if module_name.ends_with(".ts") {
        Some(".ts")
    } else if module_name.ends_with(".tsx") {
        Some(".tsx")
    } else if module_name.ends_with(".mts") {
        Some(".mts")
    } else if module_name.ends_with(".cts") {
        Some(".cts")
    } else {
        None
    }
}

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Import Declaration Validation
    // =========================================================================

    /// TS1214: Check import binding names for strict-mode reserved words.
    /// Import declarations make the file a module (always strict mode), so TS1214 applies.
    fn check_import_binding_reserved_words(&mut self, import_clause_idx: NodeIndex) {
        use crate::state_checking::is_strict_mode_reserved_name;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(clause_node) = self.ctx.arena.get(import_clause_idx) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };

        // Check default import name: `import package from "./mod"`
        if clause.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(clause.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && is_strict_mode_reserved_name(&ident.escaped_text)
        {
            self.emit_module_strict_mode_reserved_word_error(clause.name, &ident.escaped_text);
        }

        // Check named bindings (namespace import or named imports)
        if clause.named_bindings.is_none() {
            return;
        }
        let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
            return;
        };

        if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
            // `import * as package from "./mod"` — check the alias name
            if let Some(ns_data) = self.ctx.arena.get_named_imports(bindings_node)
                && ns_data.name.is_some()
                && let Some(name_node) = self.ctx.arena.get(ns_data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && is_strict_mode_reserved_name(&ident.escaped_text)
            {
                self.emit_module_strict_mode_reserved_word_error(ns_data.name, &ident.escaped_text);
            }
        } else if bindings_node.kind == syntax_kind_ext::NAMED_IMPORTS {
            // `import { foo as package } from "./mod"` — check each specifier's local name
            if let Some(named_data) = self.ctx.arena.get_named_imports(bindings_node) {
                let elements: Vec<_> = named_data.elements.nodes.to_vec();
                for elem_idx in elements {
                    let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(spec) = self.ctx.arena.get_specifier(elem_node) else {
                        continue;
                    };
                    // The local binding name is `spec.name`
                    let name_to_check = spec.name;
                    if let Some(name_node) = self.ctx.arena.get(name_to_check)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        && is_strict_mode_reserved_name(&ident.escaped_text)
                    {
                        self.emit_module_strict_mode_reserved_word_error(
                            name_to_check,
                            &ident.escaped_text,
                        );
                    }
                }
            }
        }
    }

    /// TS2823: Check that import attributes are only used with supported module options.
    pub(crate) fn check_import_attributes_module_option(&mut self, attributes_idx: NodeIndex) {
        use tsz_common::common::ModuleKind;

        if attributes_idx.is_none() {
            return;
        }

        let supported = matches!(
            self.ctx.compiler_options.module,
            ModuleKind::ESNext | ModuleKind::Node16 | ModuleKind::NodeNext | ModuleKind::Preserve
        );

        if !supported && let Some(attr_node) = self.ctx.arena.get(attributes_idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_position(
                attr_node.pos,
                attr_node.end.saturating_sub(attr_node.pos),
                diagnostic_messages::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD,
                diagnostic_codes::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD,
            );
        }
    }

    /// Check an import declaration for unresolved modules and missing exports.
    pub(crate) fn check_import_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        // TS2823: Import attributes require specific module options
        self.check_import_attributes_module_option(import.attributes);

        // TS1214/TS1212: Check import binding names for strict mode reserved words.
        // Import declarations make the file a module, so it's always strict mode → TS1214.
        self.check_import_binding_reserved_words(import.import_clause);

        if import.import_clause.is_some() {
            self.check_import_declaration_conflicts(stmt_idx, import.import_clause);
        }

        if !self.ctx.report_unresolved_imports {
            return;
        }

        // Extract module specifier data eagerly to avoid borrow issues later
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
        let has_import_clause = self.ctx.arena.get(import_clause_idx).is_some();
        let is_side_effect_import = !has_import_clause;
        if is_side_effect_import && !self.ctx.compiler_options.no_unchecked_side_effect_imports {
            return;
        }
        let is_type_only_import = self
            .ctx
            .arena
            .get(import_clause_idx)
            .and_then(|clause_node| self.ctx.arena.get_import_clause(clause_node))
            .is_some_and(|clause| clause.is_type_only);
        let mut emitted_dts_import_error = false;
        let dts_ext = if module_name.ends_with(".d.ts") {
            Some((".d.ts", ".ts", ".js"))
        } else if module_name.ends_with(".d.mts") {
            Some((".d.mts", ".mts", ".mjs"))
        } else if module_name.ends_with(".d.cts") {
            Some((".d.cts", ".cts", ".cjs"))
        } else {
            None
        };
        if let Some((dts_suffix, ts_ext, js_ext)) = dts_ext
            && !is_type_only_import
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let base = module_name.trim_end_matches(dts_suffix);
            let ext = if self.ctx.compiler_options.allow_importing_ts_extensions {
                ts_ext
            } else {
                js_ext
            };
            let suggested = format!("{base}{ext}");
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
            emitted_dts_import_error = true;
        }

        // TS5097: Check for .ts/.tsx/.mts/.cts extensions when allowImportingTsExtensions is disabled
        if !self.ctx.compiler_options.allow_importing_ts_extensions
            && !is_type_only_import
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
        }

        if let Some(binders) = &self.ctx.all_binders
            && binders.iter().any(|binder| {
                binder.declared_modules.contains(module_name)
                    || binder.shorthand_ambient_modules.contains(module_name)
            })
        {
            tracing::trace!(%module_name, "check_import_declaration: found in declared/shorthand modules, returning");
            return;
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

        // Check ambient modules BEFORE resolution errors.
        // `declare module "x"` in .d.ts files should suppress TS2307 even when
        // file-based resolution fails (matching check_import_equals_declaration).
        if self.is_ambient_module_match(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: ambient module match, returning");
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        // This must be checked before resolved_modules to catch extensionless import errors
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let mut error_code = error.code;
            let mut error_message = error.message.clone();
            if error_code
                == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                || error_code == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
            {
                // Side-effect imports use TS2882 instead of TS2307/TS2792
                if is_side_effect_import {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                    error_code = diagnostic_codes::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF;
                    error_message = format_message(
                        diagnostic_messages::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
                        &[module_name],
                    );
                } else {
                    let (fallback_message, fallback_code) = self.module_not_found_diagnostic(module_name);
                    error_code = fallback_code;
                    error_message = fallback_message;
                }
            }
            tracing::trace!(%module_name, error_code, "check_import_declaration: resolution error found");
            // Check if we've already emitted an error for this module (prevents duplicate emissions)
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx
                    .modules_with_ts2307_emitted
                    .insert(module_key.clone());
                self.error_at_position(spec_start, spec_length, &error_message, error_code);
            }
            if error_code
                != crate::diagnostics::diagnostic_codes::MODULE_WAS_RESOLVED_TO_BUT_JSX_IS_NOT_SET
            {
                self.ctx.import_resolution_stack.pop();
                return;
            }
        }

        // Check if module was successfully resolved
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
                let mut skip_export_checks = false;
                // Extract data we need before any mutable borrows
                let (is_declaration_file_flag, file_info) = {
                    let arena = self.ctx.get_arena_for_file(target_idx as u32);
                    if let Some(source_file) = arena.source_files.first() {
                        let file_name = source_file.file_name.as_str();
                        let is_js_like = file_name.ends_with(".js")
                            || file_name.ends_with(".jsx")
                            || file_name.ends_with(".mjs")
                            || file_name.ends_with(".cjs");
                        let skip_exports = is_js_like && !source_file.is_declaration_file;
                        let target_is_esm =
                            file_name.ends_with(".mjs") || file_name.ends_with(".mts");
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

                    // TS1479: Check if CommonJS file is importing an ES module
                    // This error occurs when the current file will emit require() calls
                    // but the target file is an ES module (which cannot be required)
                    let current_is_commonjs = {
                        let current_file = &self.ctx.file_name;
                        // .cts files are always CommonJS
                        let is_commonjs_file = current_file.ends_with(".cts");
                        // .mts files are always ESM
                        let is_esm_file = current_file.ends_with(".mts");
                        // For other files, check if module system will emit require() calls
                        is_commonjs_file
                            || (!is_esm_file && !self.ctx.compiler_options.module.is_es_module())
                    };

                    if current_is_commonjs && target_is_esm && !is_type_only_import {
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
                }

                if is_declaration_file_flag && !is_type_only_import && !emitted_dts_import_error {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let (base, ts_ext, js_ext) = if module_name.ends_with(".d.ts") {
                        (module_name.trim_end_matches(".d.ts"), ".ts", ".js")
                    } else if module_name.ends_with(".d.mts") {
                        (module_name.trim_end_matches(".d.mts"), ".mts", ".mjs")
                    } else if module_name.ends_with(".d.cts") {
                        (module_name.trim_end_matches(".d.cts"), ".cts", ".cjs")
                    } else {
                        (module_name.as_str(), ".ts", ".js")
                    };
                    let ext = if self.ctx.compiler_options.allow_importing_ts_extensions {
                        ts_ext
                    } else {
                        js_ext
                    };
                    let suggested = format!("{base}{ext}");
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
                }
                if let Some(binder) = self.ctx.get_binder_for_file(target_idx) {
                    let normalized_module_name = module_name.trim_matches('"').trim_matches('\'');
                    if !binder.is_external_module
                        && !self.is_ambient_module_match(module_name)
                        && !binder.declared_modules.contains(normalized_module_name)
                    {
                        let arena = self.ctx.get_arena_for_file(target_idx as u32);
                        if let Some(source_file) = arena.source_files.first()
                            && !source_file.is_declaration_file
                        {
                            let file_name = source_file.file_name.as_str();
                            let is_js_like = file_name.ends_with(".js")
                                || file_name.ends_with(".jsx")
                                || file_name.ends_with(".mjs")
                                || file_name.ends_with(".cjs");
                            if !is_js_like {
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
                }
                if !skip_export_checks {
                    self.check_imported_members(import, module_name);
                }
            } else {
                self.check_imported_members(import, module_name);
            }

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        if self.ctx.binder.module_exports.contains_key(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: found in module_exports, checking members");
            self.check_imported_members(import, module_name);

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        tracing::trace!(%module_name, "check_import_declaration: fallback - emitting module-not-found error");
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

    // =========================================================================
    // Re-export Cycle Detection
    // =========================================================================

    /// Check re-export chains for circular dependencies.
    pub(crate) fn check_reexport_chain_for_cycles(
        &mut self,
        module_name: &str,
        visited: &mut FxHashSet<String>,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if visited.contains(module_name) {
            let cycle_path: Vec<&str> = visited
                .iter()
                .map(std::string::String::as_str)
                .chain(std::iter::once(module_name))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!(
                "{}: {}",
                diagnostic_messages::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                cycle_str
            );

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error(
                    0,
                    0,
                    message,
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
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
    ) -> bool {
        if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
            let mut visited = rustc_hash::FxHashSet::default();
            return self.resolve_import_in_file(target_idx, import_name, &mut visited);
        }
        false
    }

    /// Try to resolve an import by searching all binders' re-export chains.
    pub(crate) fn resolve_import_via_all_binders(
        &self,
        module_name: &str,
        normalized: &str,
        import_name: &str,
    ) -> bool {
        if let Some(all_binders) = &self.ctx.all_binders {
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
        }
        false
    }

    /// Resolve an import by checking a specific file's exports and following
    /// re-export chains across binder boundaries. Each file has its own binder
    /// in multi-file mode, so we traverse wildcard/named re-exports by resolving
    /// each source specifier to its target file and checking that file's binder.
    fn resolve_import_in_file(
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
        if let Some(exports) = target_binder.module_exports.get(&target_file_name)
            && exports.has(import_name)
        {
            return true;
        }

        // Check named re-exports
        if let Some(reexports) = target_binder.reexports.get(&target_file_name)
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
        if let Some(source_modules) = target_binder.wildcard_reexports.get(&target_file_name) {
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

    fn check_import_declaration_conflicts(&mut self, stmt_idx: NodeIndex, clause_idx: NodeIndex) {
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
            bindings_to_check.push((clause_idx, clause.name));
        }

        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings)
        {
            if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                if let Some(ns) = self.ctx.arena.get_named_imports(bindings_node)
                    && ns.name.is_some()
                {
                    bindings_to_check.push((clause.named_bindings, ns.name));
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
                        if name_idx.is_some() {
                            bindings_to_check.push((spec_idx, name_idx));
                        }
                    }
                }
            }
        }

        for (binding_node_idx, name_idx) in bindings_to_check {
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

                        let mut import_has_value = false;
                        let mut visited = Vec::new();
                        if let Some(resolved_id) = self.resolve_alias_symbol(sym_id, &mut visited)
                            && let Some(resolved_sym) = self
                                .ctx
                                .binder
                                .get_symbol_with_libs(resolved_id, &self.get_lib_binders())
                        {
                            let mut has_value = (resolved_sym.flags
                                & (symbol_flags::VALUE | symbol_flags::EXPORT_VALUE))
                                != 0;
                            if has_value
                                && (resolved_sym.flags & symbol_flags::VALUE_MODULE) != 0
                                && (resolved_sym.flags
                                    & (symbol_flags::VALUE & !symbol_flags::VALUE_MODULE))
                                    == 0
                            {
                                let mut any_instantiated = false;
                                for &decl_idx in &resolved_sym.declarations {
                                    if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                        if decl_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION {
                                                        if self.is_namespace_declaration_instantiated(decl_idx) {
                                                            any_instantiated = true;
                                                            break;
                                                        }
                                                    } else {
                                                        any_instantiated = true;
                                                        break;
                                                    }
                                    }
                                }
                                has_value = any_instantiated;
                            }
                            import_has_value = has_value;
                            if (resolved_sym.flags & symbol_flags::ALIAS) != 0
                                && sym.import_module.is_some()
                                && sym.import_name.is_none()
                            {
                                import_has_value = true;
                            }
                        }
                        if !import_has_value {
                            continue;
                        }

                        let import_scope = self
                            .ctx
                            .binder
                            .find_enclosing_scope(self.ctx.arena, binding_node_idx);

                        // Check 1: merged declarations on the import's own symbol
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

                            let in_same_scope = if let Some(import_scope_id) = import_scope {
                                self.ctx
                                    .binder
                                    .find_enclosing_scope(self.ctx.arena, decl_idx)
                                    == Some(import_scope_id)
                            } else {
                                true
                            };
                            if !in_same_scope {
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
                                ) {
                                    return false;
                                }
                                // Check if the local declaration has value semantics
                                if let Some(flags) =
                                    self.declaration_symbol_flags(self.ctx.arena, decl_idx)
                                {
                                    (flags & symbol_flags::VALUE) != 0
                                } else {
                                    true
                                }
                            } else {
                                false
                            }
                        });

                        // Check 2: separate symbols with the same name (binder may
                        // create distinct symbols instead of merging declarations).
                        if !has_conflict {
                            let all_symbols = self.ctx.binder.symbols.find_all_by_name(&name);
                            for other_sym_id in all_symbols {
                                if other_sym_id == sym_id {
                                    continue;
                                }
                                if let Some(other_sym) = self.ctx.binder.symbols.get(other_sym_id) {
                                    if (other_sym.flags & symbol_flags::VALUE) == 0 {
                                        continue;
                                    }
                                    // Must have a declaration in the same scope
                                    let decl_in_same_scope =
                                        other_sym.declarations.iter().any(|&decl_idx| {
                                            if let Some(import_scope_id) = import_scope {
                                                self.ctx
                                                    .binder
                                                    .find_enclosing_scope(self.ctx.arena, decl_idx)
                                                    == Some(import_scope_id)
                                            } else {
                                                true
                                            }
                                        });
                                    if !decl_in_same_scope {
                                        continue;
                                    }
                                    // Must be in the current file
                                    let has_local_decl =
                                        other_sym.declarations.iter().any(|&decl_idx| {
                                            self.ctx.binder.node_symbols.get(&decl_idx.0)
                                                == Some(&other_sym_id)
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
                                name_idx,
                                &message,
                                diagnostic_codes::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                            );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ts_extension_suffix;

    #[test]
    fn ts_extension_detects_ts() {
        assert_eq!(ts_extension_suffix("./foo.ts"), Some(".ts"));
    }

    #[test]
    fn ts_extension_detects_tsx() {
        assert_eq!(ts_extension_suffix("./foo.tsx"), Some(".tsx"));
    }

    #[test]
    fn ts_extension_detects_mts() {
        assert_eq!(ts_extension_suffix("./foo.mts"), Some(".mts"));
    }

    #[test]
    fn ts_extension_detects_cts() {
        assert_eq!(ts_extension_suffix("./foo.cts"), Some(".cts"));
    }

    #[test]
    fn ts_extension_ignores_dts() {
        assert_eq!(ts_extension_suffix("./foo.d.ts"), None);
    }

    #[test]
    fn ts_extension_ignores_d_mts() {
        assert_eq!(ts_extension_suffix("./foo.d.mts"), None);
    }

    #[test]
    fn ts_extension_ignores_d_cts() {
        assert_eq!(ts_extension_suffix("./foo.d.cts"), None);
    }

    #[test]
    fn ts_extension_ignores_js() {
        assert_eq!(ts_extension_suffix("./foo.js"), None);
    }

    #[test]
    fn ts_extension_ignores_no_ext() {
        assert_eq!(ts_extension_suffix("./foo"), None);
    }

    #[test]
    fn ts_extension_ignores_json() {
        assert_eq!(ts_extension_suffix("./data.json"), None);
    }
}
