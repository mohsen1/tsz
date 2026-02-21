//! Module import/export validation and circular re-export detection.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_solver::Visibility;

// =============================================================================
// Module and Import Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Dynamic Import Validation
    // =========================================================================

    /// Check dynamic import module specifier for unresolved modules.
    ///
    /// Validates that the module specifier in a dynamic `import()` call
    /// can be resolved. Emits TS2307 if the module cannot be found.
    ///
    /// ## Parameters:
    /// - `call`: The call expression node for the `import()` call
    ///
    /// ## Validation:
    /// - Only checks string literal specifiers (dynamic specifiers cannot be statically checked)
    /// - Checks if module exists in `resolved_modules`, `module_exports`, `shorthand_ambient_modules`, or `declared_modules`
    /// - Emits TS2307 for unresolved module specifiers
    /// - Validates `CommonJS` vs ESM import compatibility
    pub(crate) fn check_dynamic_import_module_specifier(
        &mut self,
        call: &tsz_parser::parser::node::CallExprData,
    ) {
        use crate::diagnostics::diagnostic_codes;

        if !self.ctx.report_unresolved_imports {
            return;
        }

        // Get the first argument (module specifier)
        let args = match call.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };

        if args.is_empty() {
            return; // No argument - will be caught by argument count check
        }

        let arg_idx = args[0];
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return;
        };

        // Only check string literal module specifiers
        // Dynamic specifiers (variables, template literals) cannot be statically checked
        let Some(literal) = self.ctx.arena.get_literal(arg_node) else {
            return;
        };

        let module_name = &literal.text;

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return; // Module exists
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name) {
            return; // Module exists
        }

        // Check if this is a shorthand ambient module (declare module "foo")
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return; // Ambient module exists
        }

        // Check declared modules (regular ambient modules with body)
        if self.ctx.binder.declared_modules.contains(module_name) {
            return; // Declared module exists
        }

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let error_code = error.code;
            let error_message = error.message.clone();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_node(arg_idx, &error_message, error_code);
            }
            return;
        }

        // Fallback: Module not found - emit TS2307
        // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
        let module_key = module_name.to_string();
        if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            self.ctx.modules_with_ts2307_emitted.insert(module_key);
            self.error_at_node_msg(
                arg_idx,
                diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                &[module_name],
            );
        }
    }

    // =========================================================================
    // Export Module Specifier Validation
    // =========================================================================

    /// Check export declaration module specifier for unresolved modules.
    ///
    /// Validates that the module specifier in an export ... from "module" statement
    /// can be resolved. Emits TS2307 if the module cannot be found.
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The export declaration statement node
    ///
    /// ## Validation:
    /// - Checks if module exists in `resolved_modules`, `module_exports`, `shorthand_ambient_modules`, or `declared_modules`
    /// - Emits TS2307 for unresolved module specifiers
    /// - Validates re-exported members exist in source module
    /// - Checks for circular re-export chains
    pub(crate) fn check_export_module_specifier(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(export_decl) = self.ctx.arena.get_export_decl(node) else {
            return;
        };

        // Get module specifier string
        let Some(spec_node) = self.ctx.arena.get(export_decl.module_specifier) else {
            return;
        };

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;

        // Check for circular re-exports
        if self.would_create_cycle(module_name) {
            let cycle_path: Vec<&str> = self
                .ctx
                .import_resolution_stack
                .iter()
                .map(std::string::String::as_str)
                .chain(std::iter::once(module_name.as_str()))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!("Circular re-export detected: {cycle_str}");

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_node(
                    export_decl.module_specifier,
                    &message,
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
            return;
        }

        // Track re-export for cycle detection
        self.ctx.import_resolution_stack.push(module_name.clone());

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            self.check_export_target_is_module(export_decl.module_specifier, module_name);
            // Check for circular re-export chains
            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }
            // Validate named re-exports exist in target module
            self.validate_reexported_members(export_decl, module_name);
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name) {
            self.check_export_target_is_module(export_decl.module_specifier, module_name);
            // Check for circular re-export chains
            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }
            // Validate named re-exports exist in target module
            self.validate_reexported_members(export_decl, module_name);
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Skip TS2307 for ambient module declarations
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        if self.ctx.binder.declared_modules.contains(module_name) {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Emit module-not-found diagnostic for unresolved export specifiers.
        // Prefer the driver-provided resolution error (TS2792/TS2834/TS2835/etc.)
        // and dedupe through modules_with_ts2307_emitted to avoid duplicate reports.
        let module_key = module_name.to_string();
        if self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            self.ctx.import_resolution_stack.pop();
            return;
        }

        if self.ctx.get_resolution_error(module_name).is_some() {
            let (message, code) = self.module_not_found_diagnostic(module_name);
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_node(export_decl.module_specifier, &message, code);
            }
            self.ctx.import_resolution_stack.pop();
            return;
        }

        self.ctx.modules_with_ts2307_emitted.insert(module_key);
        self.error_at_node_msg(
            export_decl.module_specifier,
            diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
            &[module_name],
        );

        self.ctx.import_resolution_stack.pop();
    }

    fn check_export_target_is_module(
        &mut self,
        module_specifier_idx: NodeIndex,
        module_name: &str,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let Some(target_idx) = self.ctx.resolve_import_target(module_name) else {
            return;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_idx) else {
            return;
        };
        if target_binder.is_external_module
            || self.is_ambient_module_match(module_name)
            || target_binder
                .declared_modules
                .contains(module_name.trim_matches('"').trim_matches('\''))
        {
            return;
        }
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return;
        };
        if source_file.is_declaration_file {
            return;
        }
        let file_name = source_file.file_name.as_str();
        let is_js_like = file_name.ends_with(".js")
            || file_name.ends_with(".jsx")
            || file_name.ends_with(".mjs")
            || file_name.ends_with(".cjs");
        if is_js_like {
            return;
        }
        let source_file_name = source_file.file_name.clone();
        self.error_at_node_msg(
            module_specifier_idx,
            diagnostic_codes::FILE_IS_NOT_A_MODULE,
            &[&source_file_name],
        );
    }

    /// Validate that named re-exports exist in the target module.
    ///
    /// For `export { foo, bar as baz } from './module'`, validates that
    /// `foo` and `bar` are actually exported by './module'.
    ///
    /// ## Emits TS2305 when:
    /// - A named re-export doesn't exist in the target module
    fn validate_reexported_members(
        &mut self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
        module_name: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        // Only validate named exports (not wildcard exports or declarations)
        if export_decl.export_clause.is_none() {
            return;
        }

        let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
            return;
        };

        // Only check NAMED_EXPORTS (export { x, y } from 'module')
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            return;
        }

        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };

        // Get the module's canonical export surface.
        let module_exports = self.resolve_effective_module_exports(module_name);

        let Some(module_exports) = module_exports else {
            return; // Module exports not found - TS2307 already emitted
        };

        // Check each export specifier
        for &specifier_idx in &named_exports.elements.nodes {
            let Some(spec_node) = self.ctx.arena.get(specifier_idx) else {
                continue;
            };

            let Some(specifier) = self.ctx.arena.get_specifier(spec_node) else {
                continue;
            };

            // Skip type-only re-exports since they might reference types that
            // don't appear in the exports table
            if specifier.is_type_only {
                continue;
            }

            // Get the property name (what we're exporting from the source module)
            // For `export { bar as baz }`, property_name is "bar"
            // For `export { foo }`, we use the name "foo"
            let export_name = if !specifier.property_name.is_none() {
                if let Some(text) = self.get_identifier_text_from_idx(specifier.property_name) {
                    text
                } else {
                    continue;
                }
            } else if !specifier.name.is_none() {
                if let Some(text) = self.get_identifier_text_from_idx(specifier.name) {
                    text
                } else {
                    continue;
                }
            } else {
                continue;
            };

            // Check if this name is exported from the source module
            if export_name != "*" && !module_exports.has(&export_name) {
                if module_exports.has("default") || module_exports.has("export=") {
                    // TS2614: Symbol doesn't exist but a default export does
                    let message = format_message(
                        diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                        &[module_name, &export_name],
                    );
                    self.error_at_node(
                        specifier_idx,
                        &message,
                        diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                    );
                } else {
                    // TS2305: Module has no exported member
                    let message = format_message(
                        diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                        &[module_name, &export_name],
                    );
                    self.error_at_node(
                        specifier_idx,
                        &message,
                        diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                    );
                }
            }
        }
    }

    // =========================================================================
    // Dynamic Import Return Type
    // =========================================================================

    /// Get the return type for a dynamic `import()` call.
    ///
    /// Returns Promise<ModuleType> where `ModuleType` is an object containing
    /// all the module's exports. Falls back to Promise<any> or just `any` when:
    /// - The module cannot be resolved
    /// - Promise is not available (ES5 target without lib)
    ///
    /// This method implements Phase 1.3 of the module resolution plan.
    pub(crate) fn get_dynamic_import_type(
        &mut self,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> tsz_solver::TypeId {
        use tsz_solver::PropertyInfo;

        // Get the first argument (module specifier)
        let args = match call.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };

        if args.is_empty() {
            return self.create_promise_any();
        }

        let arg_idx = args[0];
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return self.create_promise_any();
        };

        // Only handle string literal module specifiers
        let Some(literal) = self.ctx.arena.get_literal(arg_node) else {
            return self.create_promise_any();
        };

        let module_name = &literal.text;

        // Check for shorthand ambient modules - imports are typed as `any`
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return self.create_promise_any();
        }

        // Try to get module exports for the namespace type.
        let exports_table = self.resolve_effective_module_exports(module_name);

        if let Some(exports_table) = exports_table {
            // Create an object type with all module exports
            let mut props: Vec<PropertyInfo> = Vec::new();
            for (name, &export_sym_id) in exports_table.iter() {
                let prop_type = self.get_type_of_symbol(export_sym_id);
                let name_atom = self.ctx.types.intern_string(name);
                props.push(PropertyInfo {
                    name: name_atom,
                    type_id: prop_type,
                    write_type: prop_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                });
            }

            // Merge module augmentations
            // Module augmentations add interfaces/types to existing modules
            // e.g., declare module 'express' { interface Request { user?: User; } }
            if let Some(augmentations) = self.ctx.binder.module_augmentations.get(module_name) {
                for aug in augmentations {
                    // Get the type of the augmentation declaration
                    let aug_type = if aug
                        .arena
                        .as_ref()
                        .is_some_and(|arena| std::ptr::eq(arena.as_ref(), self.ctx.arena))
                    {
                        self.get_type_of_node(aug.node)
                    } else {
                        tsz_solver::TypeId::ANY
                    };
                    let name_atom = self.ctx.types.intern_string(&aug.name);

                    // Check if this augments an existing export
                    if let Some(existing) = props.iter_mut().find(|p| p.name == name_atom) {
                        // Merge types - for interfaces, this creates an intersection
                        existing.type_id = self
                            .ctx
                            .types
                            .intersection(vec![existing.type_id, aug_type]);
                        existing.write_type = existing.type_id;
                    } else {
                        // New export from augmentation
                        props.push(PropertyInfo {
                            name: name_atom,
                            type_id: aug_type,
                            write_type: aug_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                        });
                    }
                }
            }

            let factory = self.ctx.types.factory();
            let module_type = factory.object(props);
            return self.create_promise_of(module_type);
        }

        // Module not found - return Promise<any>
        self.create_promise_any()
    }

    /// Create a Promise<any> type.
    fn create_promise_any(&mut self) -> tsz_solver::TypeId {
        self.create_promise_of(tsz_solver::TypeId::ANY)
    }

    /// Create a Promise<T> type for dynamic import expressions.
    ///
    /// Uses the same type resolution path as `var p: Promise<T>` to ensure
    /// structural compatibility. Falls back to `PROMISE_BASE` without lib files.
    fn create_promise_of(&mut self, inner_type: tsz_solver::TypeId) -> tsz_solver::TypeId {
        use tsz_solver::TypeId;

        // Resolve Promise as Lazy(DefId), the same form that type annotations use.
        // `var p: Promise<T>` goes through create_lazy_type_ref → Application(Lazy(DefId), [T]).
        // We must do the same here so that `import()` returns a structurally compatible type.
        let lib_binders = self.get_lib_binders();
        let factory = self.ctx.types.factory();

        if let Some(sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs("Promise", &lib_binders)
        {
            let promise_base = self.ctx.create_lazy_type_ref(sym_id);
            return factory.application(promise_base, vec![inner_type]);
        }

        // Fallback: use synthetic PROMISE_BASE (works without lib files)
        factory.application(TypeId::PROMISE_BASE, vec![inner_type])
    }

    /// Check `export { x };` (local named exports)
    /// Emits TS2661 if exporting a non-local declaration.
    pub(crate) fn check_local_named_exports(
        &mut self,
        named_exports_idx: tsz_parser::parser::NodeIndex,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(clause_node) = self.ctx.arena.get(named_exports_idx) else {
            return;
        };
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            return;
        }

        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };

        for &specifier_idx in &named_exports.elements.nodes {
            let Some(spec_node) = self.ctx.arena.get(specifier_idx) else {
                continue;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(spec_node) else {
                continue;
            };

            // Skip type-only re-exports since they are not runtime symbols
            if specifier.is_type_only {
                continue;
            }

            let name_idx = if !specifier.property_name.is_none() {
                specifier.property_name
            } else {
                specifier.name
            };
            if name_idx.is_none() {
                continue;
            }

            let name_str = self
                .get_identifier_text_from_idx(name_idx)
                .unwrap_or_else(|| String::from("unknown"));

            let mut is_local_or_imported = false;
            if let Some(sym_id) = self.ctx.binder.file_locals.get(&name_str)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                // Check if it was actually declared or imported in THIS file
                if symbol.decl_file_idx == self.ctx.current_file_idx as u32 {
                    is_local_or_imported = true;
                }
            }

            if !is_local_or_imported {
                // If it resolves to something globally but is not local, emit TS2661
                if self.resolve_identifier_symbol(name_idx).is_some()
                    || matches!(
                        name_str.as_str(),
                        "undefined"
                            | "any"
                            | "unknown"
                            | "never"
                            | "string"
                            | "number"
                            | "boolean"
                            | "symbol"
                            | "object"
                            | "bigint"
                    )
                {
                    self.error_at_node_msg(
                        name_idx,
                        crate::diagnostics::diagnostic_codes::CANNOT_EXPORT_ONLY_LOCAL_DECLARATIONS_CAN_BE_EXPORTED_FROM_A_MODULE,
                        &[&name_str],
                    );
                } else {
                    self.error_at_node_msg(
                        name_idx,
                        crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                        &[&name_str],
                    );
                }
            }
        }
    }
    /// Eagerly checks all alias symbols in the current file for circular definitions.
    /// Emits TS2303 for any alias that circularly references itself.
    pub(crate) fn check_circular_import_aliases(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        for (id_idx, sym) in self.ctx.binder.symbols.iter().enumerate() {
            if sym.flags & symbol_flags::ALIAS != 0 {
                let sym_id = tsz_binder::SymbolId(id_idx as u32);
                let mut current_binder = self.ctx.binder;
                let mut current_sym_id = sym_id;
                let mut visited = Vec::new();
                let mut cycle_detected = false;

                for _ in 0..128 {
                    let key = (current_binder as *const _, current_sym_id.0);
                    if visited.contains(&key) {
                        if std::ptr::eq(key.0, self.ctx.binder) && key.1 == sym_id.0 {
                            cycle_detected = true;
                        }
                        break;
                    }
                    visited.push(key);

                    let curr_sym = match current_binder.symbols.get(current_sym_id) {
                        Some(s) => s,
                        None => break,
                    };

                    if curr_sym.flags & symbol_flags::ALIAS == 0 {
                        break;
                    }

                    if let Some(resolved_id) = current_binder.resolve_import_symbol(current_sym_id)
                    {
                        current_sym_id = resolved_id;
                        continue;
                    }

                    let mut found = false;
                    if let Some(ref module_name) = curr_sym.import_module {
                        let export_name = curr_sym
                            .import_name
                            .as_deref()
                            .unwrap_or(&curr_sym.escaped_name);

                        if let Some(exports) = current_binder.module_exports.get(module_name)
                            && let Some(target_sym_id) = exports.get(export_name)
                        {
                            current_sym_id = target_sym_id;
                            found = true;
                        } else if let Some(binders) = &self.ctx.all_binders {
                            for b in binders.iter() {
                                if let Some(exports) = b.module_exports.get(module_name)
                                    && let Some(target_sym_id) = exports.get(export_name)
                                {
                                    current_binder = &**b;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }

                    if !found {
                        break;
                    }
                }

                if cycle_detected {
                    let decl_idx = if !sym.value_declaration.is_none() {
                        sym.value_declaration
                    } else if let Some(first) = sym.declarations.first() {
                        *first
                    } else {
                        continue;
                    };

                    let mut error_node_idx = decl_idx;

                    if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                        if decl_node.kind == syntax_kind_ext::EXPORT_SPECIFIER
                            || decl_node.kind == syntax_kind_ext::IMPORT_SPECIFIER
                        {
                            if let Some(spec) = self.ctx.arena.get_specifier(decl_node) {
                                let name_idx = if !spec.name.is_none() {
                                    spec.name
                                } else {
                                    spec.property_name
                                };
                                if !name_idx.is_none() {
                                    error_node_idx = name_idx;
                                }
                            }
                        } else if decl_node.kind == syntax_kind_ext::IMPORT_CLAUSE
                            && let Some(import_clause) = self.ctx.arena.get_import_clause(decl_node)
                                && !import_clause.name.is_none() {
                                    error_node_idx = import_clause.name;
                                }
                    }

                    let message = format_message(
                        diagnostic_messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                        &[&sym.escaped_name],
                    );
                    self.error_at_node(
                        error_node_idx,
                        &message,
                        diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                    );
                }
            }
        }
    }
}
