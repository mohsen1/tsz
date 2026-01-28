//! Module Validation Enhancements
//!
//! This module provides enhanced validation for module resolution and imports/exports.
//!
//! Features:
//! - Circular import detection
//! - Re-export chain validation
//! - Type-only import validation
//! - Default vs named export validation
//! - Module augmentation resolution

use crate::binder::{SymbolId, symbol_flags};
use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use rustc_hash::FxHashSet;
use std::collections::HashSet;

impl<'a> CheckerState<'a> {
    /// Check if adding a module to the resolution path would create a cycle.
    fn would_create_cycle(&self, module: &str) -> bool {
        self.ctx
            .import_resolution_stack
            .contains(&module.to_string())
    }

    /// Check if a module specifier is valid and emit appropriate TS2307 errors.
    ///
    /// This function centralizes module resolution validation logic.
    ///
    /// ## Parameters:
    /// - `module_name`: The module specifier to validate
    /// - `import_node`: The import declaration node for error reporting
    ///
    /// ## Returns:
    /// - `true` if the module is valid (exists or is ambient)
    /// - `false` if the module is not found (TS2307 already emitted)
    pub(crate) fn validate_module_exists(
        &mut self,
        module_name: &str,
        import_node: NodeIndex,
    ) -> bool {
        if !self.ctx.report_unresolved_imports {
            return true; // Skip validation if not reporting unresolved imports
        }

        // Check if the module was resolved by the CLI driver
        if let Some(ref resolved) = self.ctx.resolved_modules {
            if resolved.contains(module_name) {
                return true;
            }
        }

        // Check if the module exists in the module_exports map
        if self.ctx.binder.module_exports.contains_key(module_name) {
            return true;
        }

        // Check if this is an ambient module declaration (exact or wildcard pattern match).
        // Both shorthand ambient modules (`declare module "foo"`) and regular ambient modules
        // with body (`declare module "foo" { ... }`) provide type information.
        // Also supports wildcard patterns like "*.json", "foo*bar", "*!text".
        if self.is_ambient_module_match(module_name) {
            return true;
        }

        // Module not found - emit TS2307
        self.emit_module_not_found_error(module_name, import_node);
        false
    }

    /// Validate that imported members exist in the target module.
    ///
    /// This function checks that:
    /// - Named imports correspond to actual exports
    /// - Default imports are properly exported
    /// - Namespace imports can be resolved
    ///
    /// ## Parameters:
    /// - `import_decl`: The import declaration data
    /// - `module_name`: The resolved module name
    ///
    /// ## Emits TS2305 when:
    /// - A named import doesn't exist in the module
    /// - A default import is used but the module doesn't export a default
    pub(crate) fn validate_imported_members_exist(
        &mut self,
        import_decl: &ImportDeclData,
        module_name: &str,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        // Get the module's exports
        let module_exports = match self.ctx.binder.module_exports.get(module_name) {
            Some(exports) => exports,
            None => return, // Module doesn't exist - TS2307 already emitted
        };

        // Check each import clause
        if let Some(named_bindings) = &import_decl.import_clause {
            // This is `import { x, y } from 'module'` or `import { x as y } from 'module'`
            if let Some(import_clause) = self.ctx.arena.get_import_clause(*named_bindings) {
                // Check named imports
                if let Some(named_bindings_node) = import_clause.named_bindings {
                    if let Some(named_imports) =
                        self.ctx.arena.get_named_imports(named_bindings_node)
                    {
                        for &import_specifier_idx in &named_imports.elements {
                            if let Some(import_specifier) =
                                self.ctx.arena.get_import_specifier(import_specifier_idx)
                            {
                                // Get the property name (what's being imported)
                                let import_name = if let Some(prop_name) =
                                    import_specifier.property_name
                                {
                                    if let Some(ident) = self.ctx.arena.get_identifier(&prop_name) {
                                        &ident.escaped_text
                                    } else {
                                        continue;
                                    }
                                } else {
                                    // No property name means it's not renamed
                                    if let Some(name_node) = import_specifier.name {
                                        if let Some(ident) =
                                            self.ctx.arena.get_identifier(&name_node)
                                        {
                                            &ident.escaped_text
                                        } else {
                                            continue;
                                        }
                                    } else {
                                        continue;
                                    }
                                };

                                // Check if this name is exported from the module
                                if !module_exports.contains_key(import_name) {
                                    // TS2305: Module has no exported member
                                    let message = format_message(
                                        diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                                        &[import_name, module_name],
                                    );
                                    self.error_at_node(
                                        import_specifier_idx,
                                        &message,
                                        diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                                    );
                                }
                            }
                        }
                    }
                }

                // Check default import
                if let Some(name_node) = import_clause.name {
                    let default_name =
                        if let Some(ident) = self.ctx.arena.get_identifier(&name_node) {
                            &ident.escaped_text
                        } else {
                            return;
                        };

                    // Check if "default" is exported from the module
                    if !module_exports.contains_key("default") {
                        // TS2305: Module has no exported member
                        let message = format_message(
                            diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                            &["default", module_name],
                        );
                        self.error_at_node(
                            name_node,
                            &message,
                            diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                        );
                    }
                }
            }
        } else if let Some(name_node) = import_clause {
            // This is `import x from 'module'` (default import)
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                let _default_name = &ident.escaped_text;

                // Check if "default" is exported from the module
                if !module_exports.contains_key("default") {
                    // TS2305: Module has no exported member
                    let message = format_message(
                        diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                        &["default", module_name],
                    );
                    self.error_at_node(
                        *name_node,
                        &message,
                        diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                    );
                }
            }
        }
    }

    /// Check re-export chains for circular dependencies.
    ///
    /// This function detects circular re-export patterns like:
    /// ```typescript
    /// // a.ts
    /// export * from './b';
    /// // b.ts
    /// export * from './a';
    /// ```
    ///
    /// ## Parameters:
    /// - `module_name`: The starting module
    /// - `visited`: Set of already visited modules in this chain
    ///
    /// ## Emits TS2307 when a circular re-export is detected
    pub(crate) fn check_reexport_chain_for_cycles(
        &mut self,
        module_name: &str,
        visited: &mut HashSet<String>,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        // Check if we've already visited this module in the current chain
        if visited.contains(module_name) {
            // Found a cycle!
            let cycle_path: Vec<&str> =
                visited.iter().chain(std::iter::once(module_name)).collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!("{}: {}", diagnostic_messages::CANNOT_FIND_MODULE, cycle_str);
            self.error(0, 0, message, diagnostic_codes::CANNOT_FIND_MODULE);
            return;
        }

        // Add this module to the visited set
        visited.insert(module_name.to_string());

        // Check for wildcard re-exports from this module
        if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
            for source_module in source_modules {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        // Check for named re-exports from this module
        if let Some(reexports) = self.ctx.binder.reexports.get(module_name) {
            for (source_module, _) in reexports.values() {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        // Remove this module from the visited set (backtracking)
        visited.remove(module_name);
    }

    /// Validate that a type-only import is used correctly.
    ///
    /// Type-only imports (`import type { X } from 'module'`) should only
    /// reference type declarations (interfaces, type aliases, classes).
    ///
    /// ## Parameters:
    /// - `module_name`: The module being imported from
    /// - `import_name`: The specific import name
    ///
    /// ## Emits TS1371 when a value is imported with `import type`
    pub(crate) fn validate_type_only_import(
        &mut self,
        module_name: &str,
        import_name: &str,
    ) -> bool {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Get the module's exports
        let module_exports = match self.ctx.binder.module_exports.get(module_name) {
            Some(exports) => exports,
            None => return true, // Module doesn't exist - error already emitted
        };

        // Check if the symbol exists and is type-only
        if let Some(sym_id) = module_exports.get(import_name) {
            if let Some(symbol) = self.ctx.binder.get_symbol(*sym_id) {
                // Check if this is a value-only symbol
                let has_value = symbol.flags
                    & (symbol_flags::FUNCTION
                        | symbol_flags::LET
                        | symbol_flags::CONST
                        | symbol_flags::VAR)
                    != 0;
                let has_type = symbol.flags
                    & (symbol_flags::CLASS | symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)
                    != 0;

                if has_value && !has_type {
                    // This is a value-only import used with `import type`
                    self.error(
                        0,
                        0,
                        diagnostic_messages::IMPORT_TYPE_VALUE_MISMATCH,
                        diagnostic_codes::IMPORT_TYPE_VALUE_MISMATCH,
                    );
                    return false;
                }
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_validation_constants() {
        // Test that diagnostic codes are properly defined
        use crate::checker::types::diagnostics::diagnostic_codes;
        assert_eq!(diagnostic_codes::CANNOT_FIND_MODULE, 2307);
        assert_eq!(diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER, 2305);
    }
}
