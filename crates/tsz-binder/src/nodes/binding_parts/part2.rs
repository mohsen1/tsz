impl BinderState {
    /// Declare a symbol in the current scope, merging when allowed.
    pub(crate) fn declare_symbol(
        &mut self,
        arena: &NodeArena,
        name: &str,
        flags: u32,
        declaration: NodeIndex,
        is_exported: bool,
    ) -> SymbolId {
        if let Some(existing_id) = self.current_scope.get(name) {
            // Check if the existing symbol is in the local symbol table.
            // If not (e.g., it's from a lib binder), we should create a new local symbol
            // to shadow the lib symbol with the local declaration.
            if self.symbols.get(existing_id).is_none() {
                // The existing_id is from a lib binder, not our local binder.
                // Create a new symbol in the local binder to shadow the lib symbol.
                let owned_name = name.to_string();
                let sym_id = self.symbols.alloc(flags, owned_name.clone());
                let container_sym = self
                    .scope_chain
                    .get(self.current_scope_idx)
                    .and_then(|ctx| self.get_node_symbol(ctx.container_node));
                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    let span = Self::declaration_span(arena, declaration);
                    sym.add_declaration(declaration, span);
                    if (flags & symbol_flags::VALUE) != 0 {
                        sym.set_value_declaration(declaration, span);
                    }
                    sym.is_exported = is_exported;
                    if let Some(parent_id) = container_sym {
                        sym.parent = parent_id;
                    }
                }
                // Update current_scope to point to the local symbol (shadowing)
                self.current_scope.set(owned_name.clone(), sym_id);
                // CRITICAL: Also update file_locals to shadow lib symbol in file-level scope
                // This ensures symbol resolution finds the local symbol instead of the lib one
                self.file_locals.set(owned_name.clone(), sym_id);
                Arc::make_mut(&mut self.node_symbols).insert(declaration.0, sym_id);
                self.declare_in_persistent_scope(owned_name, sym_id);
                return sym_id;
            }

            let existing_flags = self.symbols.get(existing_id).map_or(0, |s| s.flags);
            let is_js_script_function_implementation = !self.is_external_module
                && !self.in_global_augmentation
                && (flags & symbol_flags::FUNCTION) != 0
                && arena.source_files.first().is_some_and(|sf| {
                    let file_name = sf.file_name.as_str();
                    file_name.ends_with(".js")
                        || file_name.ends_with(".jsx")
                        || file_name.ends_with(".mjs")
                        || file_name.ends_with(".cjs")
                })
                && arena
                    .get(declaration)
                    .and_then(|node| arena.get_function(node))
                    .is_some_and(|func| func.body.is_some());

            // In tsc, file-scope value declarations (function, var, class) shadow
            // identically-named globals from lib files — they live in different scopes.
            // Our model merges lib symbols into the file scope, so we simulate shadowing
            // by creating a new symbol instead of merging when a user function or class
            // declaration collides with a lib-originated value symbol.
            // Note: function-scoped `var` shadowing is still intentionally disabled because
            // some inference paths rely on the merged symbol behavior in legacy code paths.
            // However, module-local `let`/`const` MUST shadow lib globals (e.g. `toString`,
            // `Infinity`) to avoid false TS2451 duplicate-variable diagnostics in external
            // modules.
            //
            // In SCRIPT mode: interfaces and namespaces merge with globals (augmentation).
            // In MODULE mode: interfaces and type aliases shadow globals (no augmentation
            // at file scope — `declare global {}` is needed for true augmentation).
            let should_shadow_lib = if self.lib_symbol_ids.contains(&existing_id) {
                if self.is_external_module && !self.in_global_augmentation {
                    // In modules, interfaces, type aliases, and import aliases shadow lib symbols
                    // (they create module-local types/bindings, not global augmentation).
                    // Functions and classes also shadow as before.
                    // ALIAS (import declarations) must shadow to prevent cross-file contamination:
                    // without this, `import self = require(...)` in two separate modules would
                    // both merge into the global lib `self` symbol, causing false TS2300 duplicates.
                    //
                    // EXCEPTION: When inside `declare global { ... }`, interfaces and other
                    // declarations should MERGE with lib symbols, not shadow. The `declare global`
                    // block explicitly requests global augmentation even in external modules.
                    (flags
                        & (symbol_flags::FUNCTION
                            | symbol_flags::CLASS
                            | symbol_flags::INTERFACE
                            | symbol_flags::TYPE_ALIAS
                            | symbol_flags::ALIAS
                            | symbol_flags::BLOCK_SCOPED_VARIABLE))
                        != 0
                } else {
                    // In scripts, class and function declarations shadow lib value
                    // symbols. tsc resolves file-scope declarations before globals,
                    // so a user `declare function print(s: string): void;` shadows
                    // the lib's `print(): void` rather than merging into overloads.
                    // JS/checkJs function implementations are the exception: tsc
                    // keeps the ambient lib signature in the overload set and checks
                    // the JS implementation against it (for example global
                    // `function toString() {}` vs lib.dom's `toString(): string`).
                    ((flags & (symbol_flags::CLASS | symbol_flags::FUNCTION)) != 0)
                        && (existing_flags & symbol_flags::VALUE) != 0
                        && (flags & (symbol_flags::INTERFACE | symbol_flags::MODULE)) == 0
                        && !is_js_script_function_implementation
                }
            } else {
                false
            };
            if should_shadow_lib {
                let owned_name = name.to_string();
                // Module-local declarations only take over the namespace they
                // occupy. `interface Symbol {}` is TYPE-only, so the lib's
                // VALUE-bearing `var Symbol: SymbolConstructor` should remain
                // visible through the shadow symbol; `const Array = 1` is
                // VALUE-only, so the lib's TYPE-bearing `interface Array<T>`
                // should remain visible. Capture the lib symbol's
                // other-namespace declarations and flags here, before the
                // shadow allocation, so we can re-attach them onto the new
                // symbol below. Without this, e.g. `let xs: Array<number>`
                // produces a spurious TS2749 because the lib type `Array<T>`
                // is gone after shadowing.
                let preserved = self.collect_preserved_lib_meaning(existing_id, flags);

                let sym_id = self.symbols.alloc(flags, owned_name.clone());
                let container_sym = self
                    .scope_chain
                    .get(self.current_scope_idx)
                    .and_then(|ctx| self.get_node_symbol(ctx.container_node));
                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    let span = Self::declaration_span(arena, declaration);
                    sym.add_declaration(declaration, span);
                    if (flags & symbol_flags::VALUE) != 0 {
                        sym.set_value_declaration(declaration, span);
                    }
                    sym.is_exported = is_exported;
                    if let Some(parent_id) = container_sym {
                        sym.parent = parent_id;
                    }
                    if let Some(preserved) = preserved.as_ref() {
                        sym.flags |= preserved.flags;
                        for &(d, span) in &preserved.declarations {
                            sym.add_declaration(d, span);
                        }
                        if let Some((vd, vd_span)) = preserved.value_declaration
                            && sym.value_declaration == NodeIndex::NONE
                        {
                            sym.set_value_declaration(vd, vd_span);
                        }
                    }
                }
                if let Some(preserved) = preserved
                    && !preserved.declarations.is_empty()
                {
                    let arenas_map = Arc::make_mut(&mut self.declaration_arenas);
                    for (d, _, lib_arenas) in &preserved.declaration_arenas {
                        arenas_map
                            .entry((sym_id, *d))
                            .or_insert_with(|| lib_arenas.clone());
                    }
                }
                self.current_scope.set(owned_name.clone(), sym_id);
                self.file_locals.set(owned_name.clone(), sym_id);
                Arc::make_mut(&mut self.node_symbols).insert(declaration.0, sym_id);
                self.declare_in_persistent_scope(owned_name, sym_id);
                return sym_id;
            }
            // In merged namespace blocks, a non-exported variable must not merge with an
            // exported variable of the same name from a prior block. In tsc, these are
            // distinct symbols: `export var Origin: Point` in block 1 and `var Origin: string`
            // in block 2 are separate — the non-exported one is a local variable that shadows
            // the exported member within that block's scope, without affecting the namespace's
            // exported type.
            let is_in_module_scope = self
                .scope_chain
                .get(self.current_scope_idx)
                .is_some_and(|ctx| ctx.container_kind == ContainerKind::Module);
            let existing_is_exported = self.symbols.get(existing_id).is_some_and(|s| s.is_exported);
            if is_in_module_scope
                && existing_is_exported
                && !is_exported
                && (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
            {
                let owned_name = name.to_string();
                let sym_id = self.symbols.alloc(flags, owned_name.clone());
                let container_sym = self
                    .scope_chain
                    .get(self.current_scope_idx)
                    .and_then(|ctx| self.get_node_symbol(ctx.container_node));
                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    let span = Self::declaration_span(arena, declaration);
                    sym.add_declaration(declaration, span);
                    if (flags & symbol_flags::VALUE) != 0 {
                        sym.set_value_declaration(declaration, span);
                    }
                    sym.is_exported = false;
                    if let Some(parent_id) = container_sym {
                        sym.parent = parent_id;
                    }
                }
                self.current_scope.set(owned_name.clone(), sym_id);
                Arc::make_mut(&mut self.node_symbols).insert(declaration.0, sym_id);
                self.declare_in_persistent_scope(owned_name, sym_id);
                return sym_id;
            }

            let can_merge = Self::can_merge_flags(existing_flags, flags);

            // Alias declarations conflict with other aliases in TypeScript's
            // symbol model. Keep the duplicate declaration as a distinct symbol
            // and make it the visible binding for later references, rather than
            // appending it to the first alias symbol. This preserves duplicate
            // diagnostics while allowing later value-bearing aliases like
            // `import M = Z.M` to shadow an earlier type-only alias
            // `import M = Z.I` in expression resolution.
            if !can_merge
                && (existing_flags & symbol_flags::ALIAS) != 0
                && (flags & symbol_flags::ALIAS) != 0
            {
                let owned_name = name.to_string();
                let sym_id = self.symbols.alloc(flags, owned_name.clone());
                let container_sym = self
                    .scope_chain
                    .get(self.current_scope_idx)
                    .and_then(|ctx| self.get_node_symbol(ctx.container_node));
                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    let span = Self::declaration_span(arena, declaration);
                    sym.add_declaration(declaration, span);
                    if (flags & symbol_flags::VALUE) != 0 {
                        sym.set_value_declaration(declaration, span);
                    }
                    sym.is_exported = is_exported;
                    if let Some(parent_id) = container_sym {
                        sym.parent = parent_id;
                    }
                }
                self.current_scope.set(owned_name.clone(), sym_id);
                Arc::make_mut(&mut self.node_symbols).insert(declaration.0, sym_id);
                self.declare_in_persistent_scope(owned_name, sym_id);
                return sym_id;
            }

            let combined_flags = if can_merge {
                existing_flags | flags
            } else {
                existing_flags
            };

            // Record merge event for debugging
            self.debugger
                .record_merge(name, existing_id, existing_flags, flags, combined_flags);

            let should_upgrade_value_decl = can_merge
                && self.should_upgrade_merged_value_declaration(
                    existing_id,
                    flags,
                    declaration,
                    arena,
                );
            let should_use_new_value_decl_for_non_merge_variable_conflict = !can_merge
                && self.should_use_new_value_declaration_for_non_merge_variable_conflict(
                    existing_id,
                    flags,
                    declaration,
                    arena,
                );

            if let Some(sym) = self.symbols.get_mut(existing_id) {
                if can_merge {
                    sym.flags |= flags;
                    if should_upgrade_value_decl {
                        sym.set_value_declaration(
                            declaration,
                            Self::declaration_span(arena, declaration),
                        );
                    }
                } else if should_use_new_value_decl_for_non_merge_variable_conflict {
                    sym.set_value_declaration(
                        declaration,
                        Self::declaration_span(arena, declaration),
                    );
                }

                sym.add_declaration(declaration, Self::declaration_span(arena, declaration));
                if is_exported {
                    sym.is_exported = true;
                }

                // Record declaration event (merge)
                self.debugger.record_declaration(
                    name,
                    existing_id,
                    combined_flags,
                    sym.declarations.len(),
                    true,
                );
            }

            Arc::make_mut(&mut self.node_symbols).insert(declaration.0, existing_id);
            self.declare_in_persistent_scope(name.to_string(), existing_id);
            return existing_id;
        }

        // For function-scoped variables (var), check if this declaration was already
        // processed during the hoisting pass. `var` declarations are hoisted to the
        // function/file scope before the main bind pass. If the current scope is a
        // block scope (e.g., for-loop), the hoisted symbol lives in a parent scope
        // and won't be found in current_scope. Look it up via node_symbols which
        // was populated during hoisting.
        if (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
            && let Some(&existing_id) = self.node_symbols.get(&declaration.0)
            && self.symbols.get(existing_id).is_some_and(|sym| {
                // Only reuse the existing symbol if it was actually hoisted as a
                // function-scoped variable. Constructor parameter properties use the
                // same AST node (the Parameter) for both the class-scope PROPERTY
                // symbol and the constructor-scope parameter. Without this check,
                // the parameter binding would incorrectly reuse the PROPERTY symbol,
                // leaking it into the function scope and causing false TS2451
                // diagnostics when a static member shares the name.
                (sym.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
            })
        {
            // Already hoisted — just ensure we don't double-add the declaration
            if let Some(sym) = self.symbols.get_mut(existing_id) {
                sym.add_declaration(declaration, Self::declaration_span(arena, declaration));
                if is_exported {
                    sym.is_exported = true;
                }
            }
            self.declare_in_persistent_scope(name.to_string(), existing_id);
            return existing_id;
        }

        // Allocate the name string once and reuse via clone for all tables.
        // This reduces per-declaration heap allocations from ~5 to ~2-3.
        let owned_name = name.to_string();
        let sym_id = self.symbols.alloc(flags, owned_name.clone());
        // Set parent to the current container's symbol (namespace, class, etc.)
        let container_sym = self
            .scope_chain
            .get(self.current_scope_idx)
            .and_then(|ctx| self.get_node_symbol(ctx.container_node));
        if let Some(sym) = self.symbols.get_mut(sym_id) {
            let span = Self::declaration_span(arena, declaration);
            sym.add_declaration(declaration, span);
            if sym.value_declaration.is_none() && (flags & symbol_flags::VALUE) != 0 {
                sym.set_value_declaration(declaration, span);
            }
            sym.is_exported = is_exported;
            if let Some(parent_id) = container_sym {
                sym.parent = parent_id;
            }
        }
        self.current_scope.set(owned_name.clone(), sym_id);

        // Keep source-file declarations visible through file_locals.
        // This is required for nested module scopes resolving references to
        // top-level ambient symbols (e.g. `import alias = demoNS` inside `declare module`).
        //
        // IMPORTANT: Do NOT add symbols from module augmentation bodies to file_locals.
        // Module augmentation declarations (`declare module "./x" { interface Foo { ... } }`)
        // are tracked separately via `module_augmentations` and merged at type resolution time.
        // Adding them to file_locals pollutes the driver's cross-file merge, causing the
        // augmentation's symbol to overwrite the original module's exported symbol.
        if self.current_scope_id.is_some()
            && !self.in_module_augmentation
            && self
                .scopes
                .get(self.current_scope_id.0 as usize)
                .is_some_and(|scope| scope.kind == ContainerKind::SourceFile)
        {
            self.file_locals.set(owned_name.clone(), sym_id);
        }

        Arc::make_mut(&mut self.node_symbols).insert(declaration.0, sym_id);
        self.declare_in_persistent_scope(owned_name, sym_id);

        // Record declaration event (new symbol)
        self.debugger
            .record_declaration(name, sym_id, flags, 1, false);

        sym_id
    }

    /// Check if two symbol flag sets can be merged.
    /// Made public for use in checker to detect duplicate identifiers (TS2300).
    #[must_use]
    pub const fn can_merge_flags(existing_flags: u32, new_flags: u32) -> bool {
        if (existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        if (existing_flags & symbol_flags::CLASS != 0 && (new_flags & symbol_flags::INTERFACE) != 0)
            || (existing_flags & symbol_flags::INTERFACE != 0
                && (new_flags & symbol_flags::CLASS) != 0)
        {
            return true;
        }

        if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags
                & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }

        // Namespace/module can merge with interface
        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        if (existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        // Allow function + class merging (TypeScript allows declare function + declare class)
        if (existing_flags & symbol_flags::FUNCTION) != 0 && (new_flags & symbol_flags::CLASS) != 0
        {
            return true;
        }
        if (existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        // Allow method overloads to merge (method signature + method implementation)
        if (existing_flags & symbol_flags::METHOD) != 0 && (new_flags & symbol_flags::METHOD) != 0 {
            return true;
        }

        // Allow VARIABLE + NAMESPACE_MODULE merging.
        // TypeScript's NamespaceModuleExcludes = 0 (can merge with anything) and
        // FunctionScopedVariableExcludes doesn't include NAMESPACE_MODULE.
        // e.g., `namespace m2 { ... } var m2: { ... };`
        if (existing_flags & symbol_flags::NAMESPACE_MODULE) != 0
            && (new_flags & symbol_flags::VARIABLE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::NAMESPACE_MODULE) != 0
            && (existing_flags & symbol_flags::VARIABLE) != 0
        {
            return true;
        }

        // Allow INTERFACE to merge with VALUE symbols (e.g., `interface Object` + `declare var Object`)
        // This enables global types like Object, Array, Promise to be used as both types and constructors
        if (existing_flags & symbol_flags::INTERFACE) != 0 && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::INTERFACE) != 0 && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow TYPE_ALIAS to merge with VALUE symbols
        // In TypeScript, type aliases and values exist in separate namespaces
        // and can share the same name:
        //   type Foo = number;
        //   export const Foo = 1;  // legal: Foo is both a type and a value
        if (existing_flags & symbol_flags::TYPE_ALIAS) != 0
            && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::TYPE_ALIAS) != 0
            && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow TYPE_PARAMETER to merge with VALUE symbols
        // e.g., `<T>(T: T) => T`
        if (existing_flags & symbol_flags::TYPE_PARAMETER) != 0
            && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::TYPE_PARAMETER) != 0
            && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow ALIAS (import) to merge with VALUE symbols.
        // In TypeScript, imports and local value declarations can share the
        // same name — the import occupies the type namespace and the local
        // declaration occupies the value namespace:
        //   import type { A } from "./a";
        //   const A: A = "a";  // legal: A is both a type and a value
        if (existing_flags & symbol_flags::ALIAS) != 0 && (new_flags & symbol_flags::VALUE) != 0 {
            return true;
        }
        if (new_flags & symbol_flags::ALIAS) != 0 && (existing_flags & symbol_flags::VALUE) != 0 {
            return true;
        }

        // Allow ALIAS (import) to merge with local type declarations.
        // Import clauses can legally share a name with interfaces/type aliases
        // and form a single merged symbol that's usable in both namespaces:
        //   export default interface Foo {}
        //   import Foo from "./mod";
        //   export { Foo as default };
        if (existing_flags & symbol_flags::ALIAS) != 0
            && (new_flags & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::ALIAS) != 0
            && (existing_flags & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)) != 0
        {
            return true;
        }

        // Allow ALIAS to merge with MODULE (namespace/module).
        // In TypeScript, AliasExcludes = Alias (only conflicts with other aliases)
        // and NamespaceModuleExcludes = 0 (can merge with anything).
        // This covers `export as namespace X` + `declare namespace X` coexisting:
        //   export = React;
        //   export as namespace React;  // creates ALIAS
        //   declare namespace React {}  // creates MODULE — must merge
        if (existing_flags & symbol_flags::ALIAS) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }
        if (new_flags & symbol_flags::ALIAS) != 0 && (existing_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

        // Allow static and instance members to have the same name
        // TypeScript allows a class to have both a static member and an instance member with the same name
        // e.g., class C { static foo; foo; }
        let existing_is_static = (existing_flags & symbol_flags::STATIC) != 0;
        let new_is_static = (new_flags & symbol_flags::STATIC) != 0;
        if existing_is_static != new_is_static {
            // One is static, one is instance - allow merge
            return true;
        }

        false
    }
}
