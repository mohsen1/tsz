//! Module import/export validation and circular re-export detection.

use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_solver::Visibility;

// =============================================================================
// Module and Import Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
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
        let (message, code) = self.module_not_found_diagnostic(module_name);
        self.error_at_node(export_decl.module_specifier, &message, code);

        self.ctx.import_resolution_stack.pop();
    }

    pub(crate) fn check_export_target_is_module(
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
        let file_name = source_file.file_name.as_str();
        let is_js_like = file_name.ends_with(".js")
            || file_name.ends_with(".jsx")
            || file_name.ends_with(".mjs")
            || file_name.ends_with(".cjs");
        let is_json_module =
            file_name.ends_with(".json") && self.ctx.compiler_options.resolve_json_module;
        if is_js_like || is_json_module {
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
        // TSC includes source-level quotes in module diagnostic messages
        let quoted_module = format!("\"{module_name}\"");

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
            let export_name = if specifier.property_name.is_some() {
                if let Some(text) = self.get_identifier_text_from_idx(specifier.property_name) {
                    text
                } else {
                    continue;
                }
            } else if specifier.name.is_some() {
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
                        &[&quoted_module, &export_name],
                    );
                    self.error_at_node(
                        specifier_idx,
                        &message,
                        diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                    );
                } else {
                    // Check for spelling suggestions (TS2724) before TS2305
                    let export_names: Vec<&str> = module_exports
                        .iter()
                        .map(|(name, _)| name.as_str())
                        .collect();
                    if let Some(suggestion) = tsz_parser::parser::spelling::get_spelling_suggestion(
                        &export_name,
                        &export_names,
                    ) {
                        // TS2724: did you mean?
                        let message = format_message(
                            diagnostic_messages::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
                            &[&quoted_module, &export_name, suggestion],
                        );
                        self.error_at_node(
                            specifier_idx,
                            &message,
                            diagnostic_codes::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
                        );
                    } else {
                        // TS2305: Module has no exported member
                        let message = format_message(
                            diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                            &[&quoted_module, &export_name],
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
            // Get export= type if this is a CommonJS module
            let export_equals_type = exports_table
                .get("export=")
                .map(|export_equals_sym| self.get_type_of_symbol(export_equals_sym));

            // Create an object type with all module exports
            let mut props: Vec<PropertyInfo> = Vec::new();
            for (name, &export_sym_id) in exports_table.iter() {
                if name == "export=" {
                    continue;
                }
                let prop_type = self.get_type_of_symbol(export_sym_id);
                let name_atom = self.ctx.types.intern_string(name);
                props.push(PropertyInfo {
                    name: name_atom,
                    type_id: prop_type,
                    write_type: prop_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
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
                        existing.type_id = self.ctx.types.intersection2(existing.type_id, aug_type);
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
                            is_class_prototype: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: 0,
                        });
                    }
                }
            }

            // When esModuleInterop / allowSyntheticDefaultImports is enabled
            // and the module uses `export =`, synthesize a `default` property
            // so that `import("./foo").then(f => f.default)` works.
            if let Some(eq_type) = export_equals_type
                && self.ctx.allow_synthetic_default_imports()
            {
                let default_atom = self.ctx.types.intern_string("default");
                if !props.iter().any(|p| p.name == default_atom) {
                    props.push(PropertyInfo {
                        name: default_atom,
                        type_id: eq_type,
                        write_type: eq_type,
                        optional: false,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: 0,
                    });
                }
            }

            let factory = self.ctx.types.factory();
            let module_type = factory.object(props);
            let display_module_name =
                self.resolve_namespace_display_module_name(&exports_table, module_name);
            self.ctx
                .namespace_module_names
                .insert(module_type, display_module_name);
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
            let _ = self.get_type_of_symbol(sym_id);
            // Ensure the Promise DefId has its type parameters and body registered
            // so that resolve_application_property can substitute T with the inner type.
            // Without this, .then() callback parameters remain as unsubstituted `T`.
            self.ensure_def_ready_for_lowering(sym_id, "Promise");
            let promise_base = self.ctx.create_lazy_type_ref(sym_id);
            return factory.application(promise_base, vec![inner_type]);
        }

        // Fallback: use synthetic PROMISE_BASE (works without lib files)
        factory.application(TypeId::PROMISE_BASE, vec![inner_type])
    }

    /// Check `export { x };` (local named exports)
    /// Emits TS2661 if exporting a non-local declaration.
    /// TS2207: The 'type' modifier cannot be used on a named export when 'export type' is
    /// used on its export statement. E.g., `export type { type X as Y }` is invalid because
    /// the specifier-level `type` modifier conflicts with the statement-level `export type`.
    pub(crate) fn check_type_modifier_on_type_only_export(
        &mut self,
        named_exports_idx: tsz_parser::parser::NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let Some(clause_node) = self.ctx.arena.get(named_exports_idx) else {
            return;
        };
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
            if specifier.is_type_only {
                self.error_at_node(
                    specifier_idx,
                    "The 'type' modifier cannot be used on a named export when 'export type' is used on its export statement.",
                    diagnostic_codes::THE_TYPE_MODIFIER_CANNOT_BE_USED_ON_A_NAMED_EXPORT_WHEN_EXPORT_TYPE_IS_USED_ON_I,
                );
            }
        }
    }

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

        // Skip local-export checks when the export is in a wrong context (inside block/function).
        // The grammar error (TS1233) is the primary error; TS2661/TS2304 shouldn't also fire.
        if self.is_in_non_module_element_context(named_exports_idx) {
            return;
        }

        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };

        // Check if the export clause is inside an ambient module declaration
        // (e.g., `declare module "m" { export { X }; }`). Inside such blocks,
        // only declarations within the module scope are local — file-level
        // declarations from the outer scope are NOT local to the module.
        let inside_ambient_module =
            self.is_inside_string_literal_module_declaration(named_exports_idx);

        let mut seen_export_names: FxHashMap<String, NodeIndex> = FxHashMap::default();

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

            let name_idx = if specifier.property_name.is_some() {
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

            // Check for duplicate exported names in the same export clause
            let export_name_str = self
                .get_identifier_text_from_idx(specifier.name)
                .unwrap_or_else(|| name_str.clone());
            match seen_export_names.entry(export_name_str.clone()) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    use tsz_common::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let msg = format_message(
                        diagnostic_messages::DUPLICATE_IDENTIFIER,
                        &[&export_name_str],
                    );
                    let code = diagnostic_codes::DUPLICATE_IDENTIFIER;
                    let first_idx = *entry.get();
                    if first_idx != NodeIndex::NONE {
                        self.error_at_node(first_idx, &msg, code);
                        *entry.get_mut() = NodeIndex::NONE;
                    }
                    self.error_at_node(specifier.name, &msg, code);
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(specifier.name);
                }
            }

            // Check if the symbol is a local declaration or import.
            // file_locals includes merged globals from other files, so we must also
            // verify decl_file_idx matches the current file (or is u32::MAX for single-file).
            // Inside ambient module declarations, file-level symbols are not local to the
            // module and should emit TS2661.
            let current_file_idx = self.ctx.current_file_idx as u32;
            let is_local = if inside_ambient_module {
                // Inside `declare module "m"`, only symbols declared within
                // the module's own scope count as local. Check the binder's
                // scope chain: walk from the specifier's scope up to the first
                // Module scope and check its symbol table.
                self.is_name_in_enclosing_module_scope(&name_str, specifier_idx)
            } else {
                self.ctx
                    .binder
                    .file_locals
                    .get(&name_str)
                    .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                    .is_some_and(|sym| {
                        sym.decl_file_idx == current_file_idx || sym.decl_file_idx == u32::MAX
                    })
            };

            if !is_local {
                // Symbol is not local to the current module/file.
                // Distinguish between accessible-but-not-local (TS2661) and truly missing (TS2304).
                let is_resolvable = self.resolve_identifier_symbol(name_idx).is_some()
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
                            | "globalThis"
                    );

                if is_resolvable {
                    self.error_at_node_msg(
                        name_idx,
                        crate::diagnostics::diagnostic_codes::CANNOT_EXPORT_ONLY_LOCAL_DECLARATIONS_CAN_BE_EXPORTED_FROM_A_MODULE,
                        &[&name_str],
                    );
                } else {
                    // Route through boundary for TS2304/TS2552 with suggestion collection
                    self.report_not_found_at_boundary(
                        &name_str,
                        name_idx,
                        crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    );
                }
            }
        }
    }
    /// Check if a node is inside an ambient module declaration with a string-literal name
    /// (e.g., `declare module "m" { ... }`). Returns false for namespace declarations
    /// (e.g., `declare namespace Foo { ... }`).
    fn is_inside_string_literal_module_declaration(
        &self,
        node_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let mut current = node_idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module_decl) = self.ctx.arena.get_module(node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(module_decl.name) else {
                continue;
            };
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                return true;
            }
        }
        false
    }

    /// Check if a name is declared within the nearest enclosing Module scope.
    /// Used inside `declare module "m"` blocks to distinguish local declarations
    /// from outer-scope symbols.
    fn is_name_in_enclosing_module_scope(
        &self,
        name: &str,
        node_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        use tsz_binder::scopes::ContainerKind;

        // Find the enclosing scope for this node
        let Some(scope_id) = self
            .ctx
            .binder
            .node_scope_ids
            .get(&node_idx.0)
            .copied()
            .or_else(|| {
                // Walk up parent nodes to find one with a scope
                let mut current = node_idx;
                loop {
                    let ext = self.ctx.arena.get_extended(current)?;
                    current = ext.parent;
                    if current.is_none() {
                        return None;
                    }
                    if let Some(&sid) = self.ctx.binder.node_scope_ids.get(&current.0) {
                        return Some(sid);
                    }
                }
            })
        else {
            return false;
        };

        // Walk up the scope chain to find the nearest Module scope
        let mut sid = scope_id;
        while sid.is_some() {
            let Some(scope) = self.ctx.binder.scopes.get(sid.0 as usize) else {
                break;
            };
            if scope.kind == ContainerKind::Module {
                // Check if the name is in this module's scope table
                return scope.table.has(name);
            }
            sid = scope.parent;
        }
        false
    }

    /// Eagerly checks all alias symbols in the current file for circular definitions.
    /// Emits TS2303 for any alias that circularly references itself.
    pub(crate) fn check_circular_import_aliases(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let mut reported_cycle_symbols = rustc_hash::FxHashSet::default();

        let is_js_file = self.ctx.is_js_file();

        // Collect ALIAS symbols only from scope tables, not from the full symbol arena.
        // After multi-file merge, the global symbol arena contains symbols from ALL files.
        // Iterating symbols.iter() would cause each file to check every file's symbols,
        // leading to duplicate TS2303 emissions. Scope tables contain only this file's symbols.
        let mut local_alias_ids: Vec<tsz_binder::SymbolId> = Vec::new();
        for scope in &self.ctx.binder.scopes {
            for (_, &sym_id) in scope.table.iter() {
                if let Some(s) = self.ctx.binder.symbols.get(sym_id)
                    && s.flags & symbol_flags::ALIAS != 0
                    && !s.is_umd_export
                {
                    local_alias_ids.push(sym_id);
                }
            }
        }
        local_alias_ids.sort_unstable_by_key(|s| s.0);
        local_alias_ids.dedup();

        for sym_id in local_alias_ids {
            let sym = match self.ctx.binder.symbols.get(sym_id) {
                Some(s) => s,
                None => continue,
            };

            if reported_cycle_symbols.contains(&sym_id) {
                continue;
            }

            // In JS files, `import x = require(...)` is TS-only syntax (TS8002).
            // tsc skips semantic analysis for such statements — skip circular check.
            if is_js_file {
                let decl_idx = if sym.value_declaration.is_some() {
                    sym.value_declaration
                } else if let Some(&first) = sym.declarations.first() {
                    first
                } else {
                    NodeIndex::NONE
                };
                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                {
                    continue;
                }
            }

            let mut current_binder = self.ctx.binder;
            let mut current_file_idx = self.ctx.current_file_idx;
            let mut current_sym_id = sym_id;
            let mut visited = Vec::new();
            let mut visited_sym_ids = Vec::new();
            let mut cycle_detected = false;

            for _ in 0..128 {
                let key = (current_file_idx, current_sym_id.0 as usize);
                if visited.contains(&key) {
                    if key.0 == self.ctx.current_file_idx && key.1 == sym_id.0 as usize {
                        // When we get an immediate self-reference (one-step cycle),
                        // it may be a self-import pattern:
                        //   export { f as g } from "./a";  // re-export
                        //   import { g } from "./b";       // self-import
                        // The binder merges both into one symbol. The self-import
                        // resolves to the merged symbol → appears circular.
                        // Don't flag it as circular if the symbol has a re-export
                        // declaration (EXPORT_SPECIFIER with a `from` clause) that
                        // points to a different module, providing a real resolution.
                        if visited.len() == 1 {
                            let has_reexport_from = sym.declarations.iter().any(|&decl_idx| {
                                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                    && decl_node.kind == syntax_kind_ext::EXPORT_SPECIFIER
                                {
                                    // Check if the parent export declaration has a module
                                    // specifier (`from "..."` clause).
                                    if let Some(ext) = self.ctx.arena.get_extended(decl_idx) {
                                        let parent = ext.parent;
                                        if let Some(parent_node) = self.ctx.arena.get(parent)
                                            && parent_node.kind == syntax_kind_ext::NAMED_EXPORTS
                                            && let Some(grandparent_ext) =
                                                self.ctx.arena.get_extended(parent)
                                        {
                                            let gp = grandparent_ext.parent;
                                            if let Some(gp_node) = self.ctx.arena.get(gp)
                                                && gp_node.kind
                                                    == syntax_kind_ext::EXPORT_DECLARATION
                                                && let Some(export_decl) =
                                                    self.ctx.arena.get_export_decl(gp_node)
                                            {
                                                return export_decl.module_specifier.is_some();
                                            }
                                        }
                                    }
                                    false
                                } else {
                                    false
                                }
                            });
                            if !has_reexport_from {
                                cycle_detected = true;
                            }
                        } else {
                            cycle_detected = true;
                        }
                    }
                    break;
                }
                visited.push(key);
                visited_sym_ids.push(current_sym_id);

                let curr_sym = match current_binder.symbols.get(current_sym_id) {
                    Some(s) => s,
                    None => break,
                };

                if curr_sym.flags & symbol_flags::ALIAS == 0 {
                    break;
                }

                let mut found = false;

                // For import aliases with import_module, use cross-file resolution
                // to properly track which file we're resolving from.
                if let Some(ref module_name) = curr_sym.import_module {
                    let export_name = curr_sym
                        .import_name
                        .as_deref()
                        .unwrap_or(&curr_sym.escaped_name);

                    // Use checker's cross-file module resolution first.
                    // This correctly resolves relative specifiers from the
                    // current file's perspective and switches to the target
                    // file's binder for subsequent resolution.
                    if let Some(target_idx) = self
                        .ctx
                        .resolve_import_target_from_file(current_file_idx, module_name)
                        && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
                    {
                        if let Some(target_sym_id) = target_binder
                            .resolve_import_with_reexports_type_only(module_name, export_name)
                            .map(|(sym_id, _)| sym_id)
                            .or_else(|| {
                                (curr_sym.import_name.is_none())
                                    .then(|| {
                                        target_binder
                                            .resolve_import_with_reexports_type_only(
                                                module_name,
                                                "export=",
                                            )
                                            .map(|(sym_id, _)| sym_id)
                                    })
                                    .flatten()
                            })
                        {
                            current_binder = target_binder;
                            current_file_idx = target_idx;
                            current_sym_id = target_sym_id;
                            found = true;
                        } else {
                            let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                            if let Some(sf) = target_arena.source_files.first()
                                && let Some(exports) =
                                    target_binder.module_exports.get(&sf.file_name)
                            {
                                if let Some(target_sym_id) = exports.get(export_name) {
                                    current_binder = target_binder;
                                    current_file_idx = target_idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                } else if let Some(target_sym_id) = exports.get("export=") {
                                    current_binder = target_binder;
                                    current_file_idx = target_idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                }
                            }
                        }
                    }

                    // Fall back to binder-level resolution (same-file or merged binder)
                    if !found
                        && let Some(resolved_id) =
                            current_binder.resolve_import_symbol(current_sym_id)
                    {
                        current_sym_id = resolved_id;
                        found = true;
                    }

                    // Try current binder's module_exports directly
                    if !found
                        && let Some(exports) = current_binder.module_exports.get(module_name)
                        && let Some(target_sym_id) = exports.get(export_name)
                    {
                        current_sym_id = target_sym_id;
                        found = true;
                    }
                    if !found
                        && let Some(exports) = current_binder.module_exports.get(module_name)
                        && let Some(target_sym_id) = exports.get("export=")
                    {
                        current_sym_id = target_sym_id;
                        found = true;
                    }

                    // Fall back to all_binders for cross-file resolution
                    if !found && let Some(binders) = &self.ctx.all_binders {
                        if let Some(file_indices) = self.ctx.files_for_module_specifier(module_name)
                        {
                            for &idx in file_indices {
                                if let Some(b) = binders.get(idx)
                                    && let Some(exports) = b.module_exports.get(module_name)
                                    && let Some(target_sym_id) = exports.get(export_name)
                                {
                                    current_binder = &**b;
                                    current_file_idx = idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                    break;
                                }
                            }
                        } else {
                            for (idx, b) in binders.iter().enumerate() {
                                if let Some(exports) = b.module_exports.get(module_name)
                                    && let Some(target_sym_id) = exports.get(export_name)
                                {
                                    current_binder = &**b;
                                    current_file_idx = idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                } else if let Some(resolved_id) =
                    current_binder.resolve_import_symbol(current_sym_id)
                {
                    // Non-import alias (e.g., import = require(...)) — use binder resolution
                    current_sym_id = resolved_id;
                    found = true;
                }

                if !found
                    && std::ptr::eq(current_binder as *const _, self.ctx.binder as *const _)
                    && curr_sym.value_declaration.is_some()
                {
                    let decl_idx = curr_sym.value_declaration;
                    if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                        && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        && let Some(import_decl) = self.ctx.arena.get_import_decl(decl_node)
                    {
                        let mut base_node = import_decl.module_specifier;
                        while let Some(node) = self.ctx.arena.get(base_node)
                            && let Some(qname) = self.ctx.arena.get_qualified_name(node)
                        {
                            base_node = qname.left;
                        }
                        if let Some(node) = self.ctx.arena.get(base_node)
                            && let Some(ident) = self.ctx.arena.get_identifier(node)
                            && let Some(target_sym_id) =
                                self.resolve_name_at_node(&ident.escaped_text, base_node)
                        {
                            current_sym_id = target_sym_id;
                            found = true;
                        }
                    }
                }

                if !found {
                    break;
                }
            }

            if cycle_detected {
                // For cross-file cycles, use max SymbolId heuristic to deduplicate:
                // only report the cycle from the file containing the highest SymbolId.
                // For same-file cycles, report on the first symbol encountered (no dedup needed).
                let this_file_idx = self.ctx.current_file_idx;
                let is_cross_file = visited.iter().any(|key| key.0 != this_file_idx);
                if is_cross_file {
                    let max_sym_in_cycle = visited_sym_ids
                        .iter()
                        .max_by_key(|s| s.0)
                        .copied()
                        .unwrap_or(sym_id);
                    if sym_id != max_sym_in_cycle {
                        continue;
                    }
                }

                for key in &visited {
                    if key.0 == this_file_idx {
                        reported_cycle_symbols.insert(tsz_binder::SymbolId(key.1 as u32));
                    }
                }

                let decl_idx = if sym.value_declaration.is_some() {
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
                            let name_idx = if spec.name.is_some() {
                                spec.name
                            } else {
                                spec.property_name
                            };
                            if name_idx.is_some() {
                                error_node_idx = name_idx;
                            }
                        }
                    } else if decl_node.kind == syntax_kind_ext::IMPORT_CLAUSE
                        && let Some(import_clause) = self.ctx.arena.get_import_clause(decl_node)
                        && import_clause.name.is_some()
                    {
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

    // =========================================================================
    // verbatimModuleSyntax / isolatedModules Export Checks (TS1205, TS1284, TS1285, TS1448)
    // =========================================================================

    /// TS1205: Re-exporting a type when 'verbatimModuleSyntax' or 'isolatedModules' is enabled
    /// requires using `export type`.
    /// TS1448: Re-exporting a type-only declaration requires type-only re-export under isolatedModules.
    pub(crate) fn check_verbatim_module_syntax_named_exports(
        &mut self,
        named_exports_idx: NodeIndex,
        module_specifier_idx: NodeIndex,
    ) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        // Determine which option is active and its name for error messages
        let option_name = if self.ctx.compiler_options.verbatim_module_syntax {
            "verbatimModuleSyntax"
        } else if self.ctx.compiler_options.isolated_modules {
            "isolatedModules"
        } else {
            return;
        };

        let Some(clause_node) = self.ctx.arena.get(named_exports_idx) else {
            return;
        };
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            return;
        }
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };

        // Get the module specifier text (if this is a re-export with `from`)
        let module_specifier_text = if module_specifier_idx.is_some() {
            self.ctx
                .arena
                .get(module_specifier_idx)
                .and_then(|n| self.ctx.arena.get_literal(n))
                .map(|l| l.text.clone())
        } else {
            None
        };

        for &specifier_idx in &named_exports.elements.nodes {
            let Some(spec_node) = self.ctx.arena.get(specifier_idx) else {
                continue;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(spec_node) else {
                continue;
            };

            if specifier.is_type_only {
                continue;
            }

            let source_name_idx = if specifier.property_name.is_some() {
                specifier.property_name
            } else {
                specifier.name
            };
            let Some(source_name) = self.get_identifier_text_from_idx(source_name_idx) else {
                continue;
            };

            // Check 1: Is the symbol inherently a type? → TS1205
            // For isolatedModules: skip symbols imported via `import type` — the import
            // already makes it syntactically clear the symbol is type-only, so re-exporting
            // without `export type` is OK. Under verbatimModuleSyntax, this is still an error.
            let is_inherent_type = if let Some(ref module_spec) = module_specifier_text {
                self.is_import_specifier_type_only(module_spec, &source_name)
            } else {
                let type_only = self.is_local_symbol_type_only(&source_name);
                if type_only
                    && option_name == "isolatedModules"
                    && self.is_local_symbol_imported_as_type_only(&source_name)
                {
                    false
                } else {
                    type_only
                }
            };

            if is_inherent_type {
                let message = format_message(
                    diagnostic_messages::RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
                    &[option_name],
                );
                self.error_at_node(
                    source_name_idx,
                    &message,
                    diagnostic_codes::RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
                );
                continue;
            }

            // Check 2: Is the symbol from a type-only declaration chain?
            // For re-exports with `from`: check cross-binder type-only chains
            // For local exports: check if the local symbol was imported through a type-only chain
            let is_type_only_chain = if let Some(ref module_spec) = module_specifier_text {
                self.is_export_type_only_across_binders(module_spec, &source_name)
            } else {
                self.is_local_symbol_from_type_only_chain(&source_name)
            };

            if is_type_only_chain {
                if option_name == "verbatimModuleSyntax" {
                    // VMS uses TS1205 for both cases
                    let message = format_message(
                        diagnostic_messages::RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
                        &[option_name],
                    );
                    self.error_at_node(
                        source_name_idx,
                        &message,
                        diagnostic_codes::RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
                    );
                } else {
                    // isolatedModules uses TS1448 for type-only chain cases
                    let export_name = self
                        .get_identifier_text_from_idx(specifier.name)
                        .unwrap_or_else(|| source_name.clone());
                    let message = format_message(
                        diagnostic_messages::RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_RE_EXPORTED_USING_A_TYPE_ONLY_RE,
                        &[&export_name, option_name],
                    );
                    self.error_at_node(
                        source_name_idx,
                        &message,
                        diagnostic_codes::RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_RE_EXPORTED_USING_A_TYPE_ONLY_RE,
                    );
                }
                continue;
            }

            // TS2748: Cannot access ambient const enums when VMS is enabled.
            // For re-exports like `export { E } from "pkg"` where E is an ambient const enum.
            if option_name == "verbatimModuleSyntax"
                && let Some(ref module_spec) = module_specifier_text
                && self.is_import_specifier_ambient_const_enum(module_spec, &source_name)
            {
                let msg = format_message(
                    diagnostic_messages::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                    &["verbatimModuleSyntax"],
                );
                self.error_at_node(
                    source_name_idx,
                    &msg,
                    diagnostic_codes::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                );
            }
        }
    }

    /// TS1269: Check `export import X = require("...")` when the target is type-only.
    /// Called when the export clause of an export declaration is an `ImportEqualsDeclaration`.
    pub(crate) fn check_export_import_equals_type_only(
        &mut self,
        export_idx: NodeIndex,
        import_clause_idx: NodeIndex,
    ) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let option_name = if self.ctx.compiler_options.verbatim_module_syntax {
            "verbatimModuleSyntax"
        } else if self.ctx.compiler_options.isolated_modules {
            "isolatedModules"
        } else {
            return;
        };

        if self.ctx.is_declaration_file() {
            return;
        }

        let Some(import_node) = self.ctx.arena.get(import_clause_idx) else {
            return;
        };
        let Some(import) = self.ctx.arena.get_import_decl(import_node) else {
            return;
        };

        if import.is_type_only {
            return;
        }

        // Get the module specifier from the require(...) call
        let require_module_specifier = self.get_require_module_specifier(import.module_specifier);
        let Some(module_spec) = require_module_specifier.as_deref() else {
            return;
        };

        // Check if the target module exports a type-only entity
        let import_name = self
            .ctx
            .arena
            .get(import.import_clause)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|ident| ident.escaped_text.clone());
        let target_is_type_only = self
            .is_import_specifier_type_only(module_spec, import_name.as_deref().unwrap_or(""))
            || self.is_module_export_equals_type_only(module_spec);

        if target_is_type_only {
            let msg = format_message(
                diagnostic_messages::CANNOT_USE_EXPORT_IMPORT_ON_A_TYPE_OR_TYPE_ONLY_NAMESPACE_WHEN_IS_ENABLED,
                &[option_name],
            );
            self.error_at_node(
                export_idx,
                &msg,
                diagnostic_codes::CANNOT_USE_EXPORT_IMPORT_ON_A_TYPE_OR_TYPE_ONLY_NAMESPACE_WHEN_IS_ENABLED,
            );
        }
    }

    /// Check if a local symbol was imported from a module where the export is type-only
    /// (e.g., the source module uses `export type { X }`), but the symbol itself is not
    /// inherently a type. This is the TS1448 case for isolatedModules.
    fn is_local_symbol_from_type_only_chain(&self, name: &str) -> bool {
        use tsz_binder::symbol_flags;

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
        {
            // If the symbol itself is type-only (import type), this is already
            // caught by is_local_symbol_type_only → TS1205
            if sym.is_type_only {
                return false;
            }
            // Follow through import alias chain to check if the source export is type-only
            if (sym.flags & symbol_flags::ALIAS) != 0
                && let Some(ref module_spec) = sym.import_module
            {
                let import_name = sym.import_name.as_deref().unwrap_or(name);
                return self.is_export_type_only_across_binders(module_spec, import_name);
            }
        }
        false
    }

    /// Check if a local symbol was imported via `import type` (directly type-only import).
    /// This is used to distinguish between symbols that are type-only because they were
    /// explicitly imported as types vs symbols that are type-only because they ARE types.
    fn is_local_symbol_imported_as_type_only(&self, name: &str) -> bool {
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
        {
            return sym.is_type_only;
        }
        false
    }

    /// Check if a local symbol is purely a type entity.
    /// Resolves through import chains: if `name` is an imported symbol,
    /// checks whether the source module's export is type-only.
    fn is_local_symbol_type_only(&self, name: &str) -> bool {
        use tsz_binder::symbol_flags;

        const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        const VALUE: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
        {
            if sym.is_type_only {
                return true;
            }
            if (sym.flags & PURE_TYPE) != 0 && (sym.flags & VALUE) == 0 {
                return true;
            }
            // If this is an imported symbol, resolve through the import chain
            // to check if the source export is purely a type.
            if (sym.flags & symbol_flags::ALIAS) != 0
                && let Some(ref module_spec) = sym.import_module
            {
                let import_name = sym.import_name.as_deref().unwrap_or(name);
                return self.is_import_specifier_type_only(module_spec, import_name);
            }
        }
        false
    }

    /// Determine if the current file is treated as CommonJS.
    fn is_current_file_commonjs(&self) -> bool {
        let current_file = &self.ctx.file_name;
        if current_file.ends_with(".cts") || current_file.ends_with(".cjs") {
            return true;
        }
        if current_file.ends_with(".mts") || current_file.ends_with(".mjs") {
            return false;
        }
        if self.ctx.compiler_options.module.is_node_module() {
            if let Some(is_esm) = self.ctx.file_is_esm {
                return !is_esm;
            }
        }
        !self.ctx.compiler_options.module.is_es_module()
    }

    /// TS1295: ESM exports cannot be written in a CommonJS file under verbatimModuleSyntax.
    /// TS1287: top-level export on value declarations in CJS.
    /// Returns true if a CJS-specific diagnostic was emitted.
    pub(crate) fn check_verbatim_module_syntax_cjs_export(
        &mut self,
        export_idx: NodeIndex,
        is_type_only: bool,
        is_value_export: bool,
    ) -> bool {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        if !self.ctx.compiler_options.verbatim_module_syntax {
            return false;
        }
        if !self.is_current_file_commonjs() {
            return false;
        }
        // Type-only exports are erased, so they're fine in CJS
        if is_type_only {
            return false;
        }
        if is_value_export {
            // TS1287: top-level export modifier on value declaration
            self.error_at_node(
                export_idx,
                diagnostic_messages::A_TOP_LEVEL_EXPORT_MODIFIER_CANNOT_BE_USED_ON_VALUE_DECLARATIONS_IN_A_COMMONJS_M,
                diagnostic_codes::A_TOP_LEVEL_EXPORT_MODIFIER_CANNOT_BE_USED_ON_VALUE_DECLARATIONS_IN_A_COMMONJS_M,
            );
        } else {
            // TS1295: ESM export syntax in CJS
            self.error_at_node(
                export_idx,
                diagnostic_messages::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2,
                diagnostic_codes::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2,
            );
        }
        true
    }

    /// TS1284/TS1285: export default checks under verbatimModuleSyntax.
    pub(crate) fn check_verbatim_module_syntax_export_default(&mut self, clause_idx: NodeIndex) {
        use tsz_binder::symbol_flags;
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if !self.ctx.compiler_options.verbatim_module_syntax {
            return;
        }

        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(clause_node) else {
            return;
        };
        let name = ident.escaped_text.clone();

        const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        const VALUE: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

        if let Some(sym_id) = self.ctx.binder.file_locals.get(&name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
        {
            // TS1285: type-only import
            if sym.is_type_only {
                let message = format_message(
                        diagnostic_messages::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABL,
                        &[&name],
                    );
                self.error_at_node(
                        clause_idx,
                        &message,
                        diagnostic_codes::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABL,
                    );
                return;
            }

            // TS1284: pure type (interface/type alias)
            if (sym.flags & PURE_TYPE) != 0 && (sym.flags & VALUE) == 0 {
                let message = format_message(
                        diagnostic_messages::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BU,
                        &[&name],
                    );
                self.error_at_node(
                        clause_idx,
                        &message,
                        diagnostic_codes::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BU,
                    );
            }
        }
    }
}
