impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn symbol_has_portability_declaration(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> bool {
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        let Some(source_arena) = binder.symbol_arenas.get(&sym_id) else {
            return false;
        };

        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                return false;
            };
            source_arena.get_type_alias(decl_node).is_some()
                || source_arena.get_function(decl_node).is_some()
                || source_arena.get_interface(decl_node).is_some()
                || source_arena.get_signature(decl_node).is_some()
                || source_arena.get_function_type(decl_node).is_some()
                || source_arena.get_variable_declaration(decl_node).is_some()
                || source_arena.get_property_decl(decl_node).is_some()
                || source_arena.get_parameter(decl_node).is_some()
        })
    }

    /// Check if an ALIAS symbol's `import_module` resolves to a NESTED
    /// (transitive) sub-node_modules package relative to the alias's own source
    /// package.  Returns `Some(from_path)` if nested, `None` otherwise.
    ///
    /// This handles the case where the standard portability check fails because
    /// the consumer file cannot see the bare specifier (e.g., `"nested"` from
    /// `entry.ts` is invisible, but from `foo/index.d.ts` it resolves to
    /// `foo/node_modules/nested`).
    pub(in crate::declaration_emitter) fn check_nested_transitive_import(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<String> {
        use std::path::{Component, Path};

        let symbol = binder.symbols.get(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
            return None;
        }
        let import_module = symbol.import_module.as_deref()?;
        // Only bare specifiers can point to nested node_modules.
        if import_module.is_empty()
            || import_module.starts_with('.')
            || import_module.starts_with('/')
        {
            return None;
        }

        // Get the source file of the alias (e.g., `r/node_modules/foo/index.d.ts`).
        let source_path = self.get_symbol_source_path(sym_id, binder)?;

        // Find the innermost `node_modules` segment in the alias's source path.
        let components: Vec<_> = Path::new(&source_path).components().collect();
        let last_nm = components.iter().rposition(
            |c| matches!(c, Component::Normal(p) if p.to_str() == Some("node_modules")),
        )?;
        let pkg_start = last_nm + 1;
        let pkg_len = if components.get(pkg_start).is_some_and(
            |c| matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@'))),
        ) {
            2
        } else {
            1
        };
        if pkg_start + pkg_len > components.len() {
            return None;
        }

        // Build the sub-node_modules prefix: e.g., `r/node_modules/foo/node_modules/`.
        let pkg_root: std::path::PathBuf = components[..pkg_start + pkg_len].iter().collect();
        let sub_nm = pkg_root.join("node_modules");
        let sub_nm_str = format!("{}/", sub_nm.to_string_lossy());

        // Check whether any entry in module_exports lives under this sub-node_modules
        // and matches the bare specifier `import_module`.
        let found = binder.module_exports.keys().any(|path| {
            let Some(rest) = path.strip_prefix(sub_nm_str.as_str()) else {
                return false;
            };
            if import_module.starts_with('@') {
                let mut parts = rest.splitn(3, '/');
                let scope = parts.next().unwrap_or("");
                let name = parts.next().unwrap_or("");
                format!("{scope}/{name}") == import_module
            } else {
                rest.split('/').next().unwrap_or("") == import_module
            }
        });

        if !found {
            return None;
        }

        // Build the `from_path` for the diagnostic: `{pkg_name}/node_modules/{import_module}`.
        let pkg_parts: Vec<&str> = components[last_nm + 1..pkg_start + pkg_len]
            .iter()
            .filter_map(|c| match c {
                Component::Normal(p) => p.to_str(),
                _ => None,
            })
            .collect();
        Some(format!(
            "{}/node_modules/{import_module}",
            pkg_parts.join("/")
        ))
    }

    /// Get the source file path for a symbol via the binder's `symbol_arenas` and `arena_to_path`.
    ///
    /// Falls back to `global_symbol_arenas` for cross-file symbols whose arenas
    /// are not in the current file's binder (e.g., imported types from `node_modules`).
    pub(in crate::declaration_emitter) fn get_symbol_source_path(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<String> {
        // Primary path: look up the symbol's arena in the pre-built mapping.
        if let Some(source_arena) = binder.symbol_arenas.get(&sym_id) {
            let arena_addr = Arc::as_ptr(source_arena) as usize;
            if let Some(path) = self.arena_to_path.get(&arena_addr) {
                return Some(path.clone());
            }
        }

        // Fall back to global symbol arenas for cross-file symbols
        if let Some(source_arena) = self.global_symbol_arenas.get(&sym_id) {
            let arena_addr = Arc::as_ptr(source_arena) as usize;
            if let Some(path) = self.arena_to_path.get(&arena_addr) {
                return Some(path.clone());
            }
        }

        // Fallback: the checker may create symbols (e.g., for resolved imports)
        // whose IDs are not in the merge-phase symbol_arenas map.  Walk the
        // symbol's declarations and check each declaration's arena against the
        // program's arena-to-path mapping.
        let symbol = binder.symbols.get(sym_id)?;
        for &decl_idx in &symbol.declarations {
            if let Some(decl_arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in decl_arenas {
                    let arena_addr = Arc::as_ptr(arena) as usize;
                    if let Some(path) = self.arena_to_path.get(&arena_addr) {
                        return Some(path.clone());
                    }
                }
            }
        }

        // Last resort: use the symbol's decl_file_idx which was set during
        // the multi-file merge phase.  This covers interface/type symbols from
        // foreign files that lack both symbol_arenas and declaration_arenas entries.
        if symbol.decl_file_idx != u32::MAX
            && let Some(path) = self.file_idx_to_path.get(&symbol.decl_file_idx)
        {
            return Some(path.clone());
        }

        None
    }
}
