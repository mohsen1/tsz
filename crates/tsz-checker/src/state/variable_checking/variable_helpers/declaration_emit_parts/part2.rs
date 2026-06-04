impl<'a> CheckerState<'a> {
    fn unique_symbol_report_target(&self, sym_id: SymbolId) -> Option<(String, SymbolId, u32)> {
        let symbol = self.get_symbol_from_any_binder(sym_id)?;
        let file_idx = symbol.decl_file_idx;
        let owner_binder = self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .unwrap_or(self.ctx.binder);

        let mut namespace_names = Vec::new();
        let mut root_namespace_sym = SymbolId::NONE;
        let mut parent_sym_id = symbol.parent;
        while parent_sym_id.is_some() {
            let Some(parent_symbol) = self.get_symbol_from_any_binder(parent_sym_id) else {
                break;
            };
            if (parent_symbol.flags
                & (tsz_binder::symbol_flags::VALUE_MODULE
                    | tsz_binder::symbol_flags::NAMESPACE_MODULE))
                == 0
            {
                break;
            }
            namespace_names.push(parent_symbol.escaped_name.clone());
            root_namespace_sym = parent_sym_id;
            parent_sym_id = parent_symbol.parent;
        }
        if !namespace_names.is_empty() {
            namespace_names.reverse();
            return Some((namespace_names.join("."), root_namespace_sym, file_idx));
        }

        let matches_symbol = |candidate_sym_id: SymbolId| {
            if candidate_sym_id == sym_id {
                return true;
            }
            let Some(candidate_symbol) = owner_binder.get_symbol(candidate_sym_id) else {
                return false;
            };
            candidate_symbol.escaped_name == symbol.escaped_name
                && (candidate_symbol.value_declaration_span == symbol.value_declaration_span
                    || candidate_symbol.first_declaration_span == symbol.first_declaration_span)
        };

        for candidate in owner_binder.symbols.iter() {
            if (candidate.flags
                & (tsz_binder::symbol_flags::VALUE_MODULE
                    | tsz_binder::symbol_flags::NAMESPACE_MODULE))
                == 0
            {
                continue;
            }
            let Some(exports) = candidate.exports.as_ref() else {
                continue;
            };
            if !exports
                .iter()
                .any(|(_, exported_sym_id)| matches_symbol(*exported_sym_id))
            {
                continue;
            }
            return Some((candidate.escaped_name.clone(), candidate.id, file_idx));
        }

        let decl_candidates = symbol.all_declarations();

        for decl_idx in decl_candidates {
            if !decl_idx.is_some() {
                continue;
            }

            let mut candidate_arenas: Vec<&tsz_parser::parser::node::NodeArena> = Vec::new();
            if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
            }
            if let Some(symbol_arena) = owner_binder.symbol_arenas.get(&sym_id) {
                candidate_arenas.push(symbol_arena.as_ref());
            }
            if std::ptr::eq(owner_binder, self.ctx.binder) {
                candidate_arenas.push(self.ctx.arena);
            }

            for arena in candidate_arenas {
                let variable_decl_idx = decl_idx;
                let Some(mut node) = arena.get(variable_decl_idx) else {
                    continue;
                };

                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    let mut parent = arena
                        .get_extended(variable_decl_idx)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                    while parent.is_some() {
                        let Some(parent_node) = arena.get(parent) else {
                            break;
                        };
                        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                            node = parent_node;
                            break;
                        }
                        parent = arena
                            .get_extended(parent)
                            .map_or(NodeIndex::NONE, |info| info.parent);
                    }
                }

                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }

                let mut namespace_names = Vec::new();
                let mut namespace_nodes = Vec::new();
                let mut parent = arena
                    .get_extended(variable_decl_idx)
                    .map_or(NodeIndex::NONE, |info| info.parent);
                while parent.is_some() {
                    let Some(parent_node) = arena.get(parent) else {
                        break;
                    };
                    if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        && let Some(module) = arena.get_module(parent_node)
                        && let Some(name_node) = arena.get(module.name)
                        && name_node.kind == SyntaxKind::Identifier as u16
                        && let Some(name_ident) = arena.get_identifier(name_node)
                    {
                        namespace_names.push(name_ident.escaped_text.clone());
                        namespace_nodes.push(parent);
                    }
                    parent = arena
                        .get_extended(parent)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                }

                if !namespace_names.is_empty() {
                    namespace_names.reverse();
                    let display_name = namespace_names.join(".");
                    let root_namespace_idx = *namespace_nodes.last().unwrap_or(&NodeIndex::NONE);
                    let root_sym_id = self
                        .ctx
                        .get_binder_for_arena(arena)
                        .and_then(|binder| binder.get_node_symbol(root_namespace_idx))
                        .unwrap_or(sym_id);
                    return Some((display_name, root_sym_id, file_idx));
                }

                return Some((symbol.escaped_name.clone(), sym_id, file_idx));
            }
        }

        Some((symbol.escaped_name.clone(), sym_id, file_idx))
    }

    fn exports_has_explicit_subpaths(exports: &serde_json::Value) -> bool {
        match exports {
            serde_json::Value::Object(map) => map.keys().any(|k| k.starts_with("./") || k == "."),
            _ => false,
        }
    }

    fn declaration_runtime_relative_path(&self, relative_path: &str) -> Option<String> {
        let relative_path = relative_path.replace('\\', "/");

        for (decl_ext, runtime_ext) in [
            (".d.ts", ".js"),
            (".d.tsx", ".jsx"),
            (".d.mts", ".mjs"),
            (".d.cts", ".cjs"),
            (".ts", ".js"),
            (".tsx", ".jsx"),
            (".mts", ".mjs"),
            (".cts", ".cjs"),
        ] {
            if let Some(prefix) = relative_path.strip_suffix(decl_ext) {
                return Some(format!("{prefix}{runtime_ext}"));
            }
        }

        Some(relative_path)
    }

    fn calculate_relative_path(&self, current: &str, source: &str) -> String {
        use std::path::{Component, Path};

        let current_path = Path::new(current);
        let source_path = Path::new(source);
        let current_dir = current_path.parent().unwrap_or(current_path);

        let current_components: Vec<_> = current_dir.components().collect();
        let source_components: Vec<_> = source_path.components().collect();

        let common_len = current_components
            .iter()
            .zip(source_components.iter())
            .take_while(|(a, b)| a == b)
            .count();

        let ups = current_components.len() - common_len;
        let mut result = String::new();
        if ups == 0 {
            result.push_str("./");
        } else {
            for _ in 0..ups {
                result.push_str("../");
            }
        }

        let remaining: Vec<_> = source_components[common_len..]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_str()?),
                _ => None,
            })
            .collect();
        result.push_str(&remaining.join("/"));

        result
    }

    fn reverse_export_specifier_for_runtime_path(
        &self,
        package_root: &std::path::Path,
        runtime_relative_path: &str,
    ) -> Option<String> {
        let package_json_path = package_root.join("package.json");
        let package_json = std::fs::read_to_string(package_json_path).ok()?;
        let package_json: serde_json::Value = serde_json::from_str(&package_json).ok()?;
        let exports = package_json.get("exports")?;
        let runtime_relative_path = format!("./{}", runtime_relative_path.trim_start_matches("./"));
        self.reverse_match_exports_subpath(exports, &runtime_relative_path)
    }

    fn reverse_match_exports_subpath(
        &self,
        exports: &serde_json::Value,
        runtime_path: &str,
    ) -> Option<String> {
        match exports {
            serde_json::Value::String(target) => {
                self.match_export_target(".", target, runtime_path)
            }
            serde_json::Value::Array(entries) => entries
                .iter()
                .find_map(|entry| self.reverse_match_exports_subpath(entry, runtime_path)),
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "." || key.starts_with("./") {
                        if let Some(specifier) =
                            self.reverse_match_export_entry(key, value, runtime_path)
                        {
                            return Some(specifier);
                        }
                        continue;
                    }

                    if let Some(specifier) = self.reverse_match_exports_subpath(value, runtime_path)
                    {
                        return Some(specifier);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn reverse_match_export_entry(
        &self,
        subpath_key: &str,
        value: &serde_json::Value,
        runtime_path: &str,
    ) -> Option<String> {
        match value {
            serde_json::Value::String(target) => {
                self.match_export_target(subpath_key, target, runtime_path)
            }
            serde_json::Value::Array(entries) => entries.iter().find_map(|entry| {
                self.reverse_match_export_entry(subpath_key, entry, runtime_path)
            }),
            serde_json::Value::Object(map) => map.values().find_map(|entry| {
                self.reverse_match_export_entry(subpath_key, entry, runtime_path)
            }),
            _ => None,
        }
    }

    fn match_export_target(
        &self,
        subpath_key: &str,
        target: &str,
        runtime_path: &str,
    ) -> Option<String> {
        let target = target.trim();
        let runtime_path = runtime_path.trim();

        if target.contains('*') {
            let wildcard = self.match_export_wildcard(target, runtime_path)?;
            return Some(self.apply_export_wildcard(subpath_key, &wildcard));
        }

        if target.ends_with('/') && subpath_key.ends_with('/') {
            let remainder = runtime_path.strip_prefix(target)?;
            return Some(format!(
                "{}{}",
                subpath_key.trim_start_matches("./"),
                remainder
            ));
        }

        if target != runtime_path {
            return None;
        }

        if subpath_key == "." {
            return Some(String::new());
        }

        Some(subpath_key.trim_start_matches("./").to_string())
    }

    fn match_export_wildcard(&self, pattern: &str, value: &str) -> Option<String> {
        let star_idx = pattern.find('*')?;
        let prefix = &pattern[..star_idx];
        let suffix = &pattern[star_idx + 1..];
        let middle = value.strip_prefix(prefix)?.strip_suffix(suffix)?;
        Some(middle.to_string())
    }

    fn apply_export_wildcard(&self, pattern: &str, wildcard: &str) -> String {
        pattern
            .replace('*', wildcard)
            .trim_start_matches("./")
            .to_string()
    }

    fn strip_ts_extensions(&self, path: &str) -> String {
        for ext in [
            ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".tsx", ".ts", ".mts", ".cts", ".jsx", ".js",
            ".mjs", ".cjs",
        ] {
            if let Some(path) = path.strip_suffix(ext) {
                return path.to_string();
            }
        }

        path.to_string()
    }

    pub(crate) fn get_symbol_from_any_binder(
        &self,
        sym_id: SymbolId,
    ) -> Option<&tsz_binder::Symbol> {
        self.ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| {
                // O(1) fast-path via resolve_symbol_file_index
                let file_idx = self.ctx.resolve_symbol_file_index(sym_id);
                if let Some(file_idx) = file_idx
                    && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
                    && let Some(sym) = binder.get_symbol(sym_id)
                {
                    return Some(sym);
                }
                self.ctx
                    .all_binders
                    .as_ref()
                    .and_then(|binders| binders.iter().find_map(|binder| binder.get_symbol(sym_id)))
            })
            .or_else(|| {
                self.ctx
                    .lib_contexts
                    .iter()
                    .find_map(|ctx| ctx.binder.get_symbol(sym_id))
            })
    }

    /// Returns true if the symbol is reachable from the current file through
    /// any module specifier already imported by the file, following named and
    /// wildcard re-export chains across files. In that case dts emit can
    /// synthesize a `typeof import("<specifier>").<name>` (or qualify through
    /// an existing alias) without requiring the symbol to have a direct local
    /// alias — matching tsc's `isSymbolAccessible` behaviour for declaration
    /// emit.
    ///
    /// This is intentionally name-agnostic: it works for any builtin or user
    /// symbol because reachability is decided by binder export tables and the
    /// checker's resolved-module map, not by matching identifier spellings in
    /// the source.
    pub(crate) fn symbol_reachable_via_local_imports(&self, target_sym_id: SymbolId) -> bool {
        if !target_sym_id.is_some() {
            return false;
        }
        let Some(target_symbol) = self.get_symbol_from_any_binder(target_sym_id) else {
            return false;
        };
        let target_name = target_symbol.escaped_name.clone();
        if target_name.is_empty() {
            return false;
        }

        let source_file_idx = self.ctx.current_file_idx;
        // The cross-file resolver lands on re-export alias symbols (the
        // `export { foo }` alias in the re-exporting file); resolve aliases
        // before comparing so the chain reaches the underlying declaration.
        let resolves_to_target = |export_name: &str, specifier: &str| -> bool {
            self.resolve_cross_file_export_from_file(specifier, export_name, Some(source_file_idx))
                .is_some_and(|resolved| {
                    let final_id = self
                        .resolve_alias_symbol(resolved, &mut AliasCycleTracker::new())
                        .unwrap_or(resolved);
                    final_id == target_sym_id
                })
        };

        let mut tried: FxHashSet<String> = FxHashSet::default();
        for (_, &local_sym_id) in self.ctx.binder.file_locals.iter() {
            let Some(local_sym) = self.ctx.binder.get_symbol(local_sym_id) else {
                continue;
            };
            if local_sym.flags & tsz_binder::symbol_flags::ALIAS == 0 {
                continue;
            }
            let Some(specifier) = local_sym.import_module.as_deref() else {
                continue;
            };
            if !tried.insert(specifier.to_string()) {
                continue;
            }

            // Fast path: the common case (no rename) is that the public
            // export name in the imported module equals the symbol's own
            // escaped name. Resolution flows through `program_module_exports`
            // and the program-wide re-export indices, which are the canonical
            // tables in the parallel pipeline (per-file binder maps can be
            // empty there).
            if resolves_to_target(&target_name, specifier) {
                return true;
            }

            // Slow path: walk every named export of the specifier. Covers
            // export-side renames (`export { internal as external }`) and
            // wildcard chains where the public name is not the symbol's own
            // escaped name. Bounded by the package's export count and gated
            // behind the fast-path miss.
            let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file(source_file_idx, specifier)
            else {
                continue;
            };
            let Some(target_binder) = self.ctx.get_binder_for_file(target_idx) else {
                continue;
            };
            let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
            let Some(target_file_name) = target_arena
                .source_files
                .first()
                .map(|sf| sf.file_name.clone())
            else {
                continue;
            };

            if let Some(module_table) = self
                .ctx
                .module_exports_for_module(target_binder, &target_file_name)
                && module_table
                    .iter()
                    .any(|(export_name, _)| resolves_to_target(export_name, specifier))
            {
                return true;
            }
        }

        false
    }

    pub(crate) fn local_value_name_resolves_to(&self, target_sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .file_locals
            .iter()
            .any(|(_, &local_sym_id)| {
                let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id) else {
                    return false;
                };
                if symbol.is_type_only {
                    return false;
                }
                // Skip symbols that came from other files via globals merge.
                // In the merged program, file_locals includes globals from all files.
                // For TS4023 "cannot be named" checks, only symbols that are actually
                // declared in or imported into the current file count as accessible.
                // A symbol from another file that ended up in globals is NOT nameable
                // in the current file's declaration emit unless it's explicitly imported.
                let is_from_current_file = symbol.decl_file_idx == u32::MAX
                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32;
                let is_import = symbol.flags & tsz_binder::symbol_flags::ALIAS != 0;
                if !is_from_current_file && !is_import {
                    return false;
                }
                if local_sym_id == target_sym_id {
                    return true;
                }

                self.ctx.binder.resolve_import_symbol(local_sym_id) == Some(target_sym_id)
            })
    }

    pub(crate) fn module_specifier_for_file(&self, file_idx: u32) -> Option<String> {
        if let Some(specifier) = self.ctx.module_specifiers.get(&file_idx) {
            return Some(specifier.clone());
        }

        let arena = self.ctx.get_arena_for_file(file_idx);
        let source_file = arena.source_files.first()?;
        let file_name = &source_file.file_name;
        let stem = file_name
            .rsplit_once('.')
            .map(|(base, _)| base)
            .unwrap_or(file_name);
        let basename = stem.rsplit_once('/').map(|(_, name)| name).unwrap_or(stem);
        Some(basename.to_string())
    }
}
