//! Module path resolution and declaration import planning helpers.

use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeArena;

impl<'a> DeclarationEmitter<'a> {
    /// Resolve a foreign symbol to its module path.
    ///
    /// Returns the module specifier (e.g., "./utils") for importing the symbol.
    pub(crate) fn resolve_symbol_module_path(&self, sym_id: SymbolId) -> Option<String> {
        let (Some(binder), Some(current_path)) = (&self.binder, &self.current_file_path) else {
            return None;
        };

        // Determine the "original" symbol (following import aliases).
        let import_resolved_sym_id = binder
            .resolve_import_symbol(sym_id)
            .filter(|resolved| *resolved != sym_id)
            .unwrap_or(sym_id);
        let original_sym_id = self
            .resolve_alias_in_source_context(sym_id, binder)
            .or_else(|| {
                if import_resolved_sym_id != sym_id {
                    self.resolve_alias_in_source_context(import_resolved_sym_id, binder)
                } else {
                    None
                }
            })
            .unwrap_or(import_resolved_sym_id);

        if let Some(symbol) = binder.symbols.get(sym_id)
            && symbol.has_any_flags(symbol_flags::ALIAS)
            && let Some(import_module) = symbol.import_module.as_deref()
            && !import_module.starts_with('.')
            && !import_module.starts_with('/')
        {
            return Some(import_module.to_string());
        }

        if let Some(path) =
            self.resolve_public_export_module_path(original_sym_id, binder, current_path)
        {
            if self.symbol_is_globally_accessible(binder, sym_id, original_sym_id) {
                return None;
            }
            return Some(path);
        }

        if let Some(path) =
            self.resolve_symbol_module_path_from_source(original_sym_id, binder, current_path)
        {
            // If the symbol is globally accessible (e.g. from a non-module .d.ts
            // or a triple-slash referenced global), suppress the import qualifier.
            if self.symbol_is_globally_accessible(binder, sym_id, original_sym_id) {
                return None;
            }
            return Some(path);
        }

        // Try the non-resolved symbol if it differs.
        if original_sym_id != sym_id
            && let Some(path) =
                self.resolve_symbol_module_path_from_source(sym_id, binder, current_path)
        {
            if self.symbol_is_globally_accessible(binder, sym_id, original_sym_id) {
                return None;
            }
            return Some(path);
        }

        // Fall back to the raw import text for imported symbols when we
        // don't have a source file mapping for the originating declaration.
        if let Some(module_specifier) = self.import_symbol_map.get(&sym_id) {
            return Some(module_specifier.clone());
        }

        binder.symbols.get(sym_id)?.import_module.clone()
    }

    fn resolve_public_export_module_path(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
        current_path: &str,
    ) -> Option<String> {
        let symbol = binder.symbols.get(sym_id)?;
        let export_name = symbol.escaped_name.as_str();
        let mut candidates = Vec::new();

        for (module_path, exports) in binder.module_exports.iter() {
            let Some(exported_sym_id) = exports.get(export_name) else {
                continue;
            };

            let exported_resolves_to_symbol = exported_sym_id == sym_id
                || binder.resolve_import_symbol(exported_sym_id) == Some(sym_id)
                || self.resolve_alias_in_source_context(exported_sym_id, binder) == Some(sym_id);
            if !exported_resolves_to_symbol {
                continue;
            }

            if self.paths_refer_to_same_source_file(current_path, module_path) {
                continue;
            }

            let module_specifier = if let Some(package_specifier) =
                self.package_specifier_for_node_modules_path(current_path, module_path)
            {
                package_specifier
            } else if let Some(package_specifier) =
                self.package_specifier_for_package_json_path(current_path, module_path)
            {
                package_specifier
            } else if let Some(package_specifier) =
                self.package_specifier_for_file_dependency_path(current_path, module_path)
            {
                package_specifier
            } else if binder.declared_modules.contains(module_path) {
                // Ambient module declaration `declare module "url" {}` — the
                // module specifier is the declared name itself, which is
                // valid wherever the declaration is reachable in scope.
                // Only kicked in when none of the path-based resolvers above
                // produced a package specifier, so we don't override existing
                // emitter behavior for declared modules that also have a
                // node_modules path (e.g. lib.* declarations).
                module_path.clone()
            } else {
                let rel_path = self.calculate_relative_path(current_path, module_path);
                self.strip_ts_extensions(&rel_path)
            };

            candidates.push(module_specifier);
        }

        for (module_path, source_modules) in binder.wildcard_reexports.iter() {
            if self.paths_refer_to_same_source_file(current_path, module_path) {
                continue;
            }

            for source_module in source_modules {
                let normalized_source_module = self.strip_ts_extensions(source_module);
                let effective_source_module = if normalized_source_module != *source_module {
                    normalized_source_module.as_str()
                } else {
                    source_module.as_str()
                };

                let reexports_symbol = self
                    .matching_module_export_paths(binder, module_path, effective_source_module)
                    .into_iter()
                    .filter_map(|source_path| binder.module_exports.get(source_path))
                    .filter_map(|exports| exports.get(export_name))
                    .any(|exported_sym_id| {
                        exported_sym_id == sym_id
                            || binder.resolve_import_symbol(exported_sym_id) == Some(sym_id)
                            || self.resolve_alias_in_source_context(exported_sym_id, binder)
                                == Some(sym_id)
                    });

                if !reexports_symbol {
                    continue;
                }

                let module_specifier = if let Some(package_specifier) =
                    self.package_specifier_for_node_modules_path(current_path, module_path)
                {
                    package_specifier
                } else if let Some(package_specifier) =
                    self.package_specifier_for_package_json_path(current_path, module_path)
                {
                    package_specifier
                } else if let Some(package_specifier) =
                    self.package_specifier_for_file_dependency_path(current_path, module_path)
                {
                    package_specifier
                } else {
                    let rel_path = self.calculate_relative_path(current_path, module_path);
                    self.strip_ts_extensions(&rel_path)
                };

                candidates.push(module_specifier);
            }
        }

        candidates.sort_by_key(|specifier| (specifier.matches('/').count(), specifier.len()));
        candidates.dedup();
        candidates.into_iter().next()
    }

    /// Check whether a foreign symbol has a local import alias in this file
    /// that will be emitted, making it referenceable by name.
    pub(in crate::declaration_emitter) fn symbol_has_local_import_alias(
        &self,
        binder: &BinderState,
        original_sym_id: SymbolId,
    ) -> bool {
        let symbol = match binder.symbols.get(original_sym_id) {
            Some(s) => s,
            None => return false,
        };
        let target_name = &symbol.escaped_name;

        // Check import_symbol_map: each entry is (alias_sym_id, module_specifier).
        // If an alias resolves to the same original symbol, the name is in scope.
        for &alias_sym_id in self.import_symbol_map.keys() {
            if let Some(resolved) = binder.resolve_import_symbol(alias_sym_id)
                && resolved == original_sym_id
            {
                return true;
            }
            // Also match by name + module when resolve_import_symbol doesn't
            // link them (e.g. cross-file merges).
            if let Some(alias_symbol) = binder.symbols.get(alias_sym_id) {
                let alias_import_name = alias_symbol
                    .import_name
                    .as_deref()
                    .unwrap_or(&alias_symbol.escaped_name);
                if alias_import_name == target_name && alias_symbol.import_module.is_some() {
                    // Verify the alias points to the same foreign module.
                    if let Some(current_path) = &self.current_file_path
                        && let Some(source_arena) = binder.symbol_arenas.get(&original_sym_id)
                    {
                        let arena_addr = std::sync::Arc::as_ptr(source_arena) as usize;
                        if let Some(source_path) = self.arena_to_path.get(&arena_addr) {
                            let rel = self.calculate_relative_path(current_path, source_path);
                            let stripped = self.strip_ts_extensions(&rel);
                            if alias_symbol.import_module.as_deref() == Some(&stripped)
                                || alias_symbol.import_module.as_deref()
                                    == Some(source_path.as_str())
                            {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Check whether a symbol is globally accessible (from a non-module .d.ts,
    /// triple-slash reference, or ambient global declaration) so it doesn't
    /// need an import("...") qualifier.
    pub(in crate::declaration_emitter) fn symbol_is_globally_accessible(
        &self,
        binder: &BinderState,
        sym_id: SymbolId,
        original_sym_id: SymbolId,
    ) -> bool {
        let check_sym_id = if original_sym_id != sym_id {
            original_sym_id
        } else {
            sym_id
        };
        let symbol = match binder.symbols.get(check_sym_id) {
            Some(s) => s,
            None => return false,
        };

        // Import aliases are never "global" in this sense.
        if symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) && symbol.import_module.is_some() {
            return false;
        }

        if let (Some(current_path), Some(source_path)) = (
            self.current_file_path.as_deref(),
            self.get_symbol_source_path(check_sym_id, binder),
        ) && !self.paths_refer_to_same_source_file(current_path, &source_path)
            && binder.module_exports.contains_key(&source_path)
        {
            return false;
        }

        // Walk up to the root parent symbol to find the top-level name.
        // For `M.C`, the root is `M`; for top-level `X`, the root is `X` itself.
        let mut root_id = check_sym_id;
        let mut root_name = &symbol.escaped_name;
        let mut cur_id = check_sym_id;
        // Walk up parent chain (max 20 levels to avoid infinite loops)
        for _ in 0..20 {
            let Some(cur_sym) = binder.symbols.get(cur_id) else {
                break;
            };
            if !cur_sym.parent.is_some() {
                root_id = cur_id;
                root_name = &cur_sym.escaped_name;
                break;
            }
            let parent_id = cur_sym.parent;
            match binder.symbols.get(parent_id) {
                Some(parent_sym) => {
                    // Symbols inside `declare module "..."` are module-scoped,
                    // not globally accessible. Return false immediately.
                    // Check: string-literal module names (starts with `"`) or
                    // MODULE-flagged parents that come from ambient module
                    // declarations (like @types/node's `declare module "url"`).
                    if parent_sym.escaped_name.starts_with('"') {
                        return false;
                    }
                    // A parent with MODULE flags whose name appears in
                    // module_exports indicates an ambient external module
                    // (e.g. `declare module "url"`). Its children are
                    // module-scoped, not globally accessible.
                    if parent_sym.has_any_flags(tsz_binder::symbol_flags::MODULE)
                        && binder.module_exports.contains_key(&parent_sym.escaped_name)
                    {
                        return false;
                    }
                    // Stop at source-file-like internal parents.
                    if parent_sym.escaped_name.starts_with("__") {
                        root_id = cur_id;
                        root_name = &cur_sym.escaped_name;
                        break;
                    }
                    cur_id = parent_id;
                }
                None => {
                    root_id = cur_id;
                    root_name = &cur_sym.escaped_name;
                    break;
                }
            }
        }

        // Check if the root symbol is accessible from file_locals or current_scope.
        self.symbol_name_is_locally_accessible(binder, root_id, root_name)
    }

    /// Check whether a symbol with the given name/id is reachable in the
    /// local scope (`file_locals` or `current_scope`) without an import qualifier.
    pub(in crate::declaration_emitter) fn symbol_name_is_locally_accessible(
        &self,
        binder: &BinderState,
        sym_id: SymbolId,
        name: &str,
    ) -> bool {
        if let Some(local_sym_id) = binder.file_locals.get(name) {
            if local_sym_id == sym_id {
                return true;
            }
            if let Some(resolved) = binder.resolve_import_symbol(local_sym_id)
                && resolved == sym_id
            {
                return true;
            }
        }
        if let Some(scope_sym_id) = binder.current_scope.get(name) {
            if scope_sym_id == sym_id {
                return true;
            }
            if let Some(resolved) = binder.resolve_import_symbol(scope_sym_id)
                && resolved == sym_id
            {
                return true;
            }
        }
        false
    }

    pub(in crate::declaration_emitter) fn resolve_symbol_module_path_from_source(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
        current_path: &str,
    ) -> Option<String> {
        let resolve_from_arena = |source_arena: &Arc<NodeArena>| {
            let arena_addr = Arc::as_ptr(source_arena) as usize;
            let source_path = self.arena_to_path.get(&arena_addr)?;
            if self.paths_refer_to_same_source_file(current_path, source_path) {
                return None;
            }

            if let Some(package_specifier) =
                self.package_specifier_for_node_modules_path(current_path, source_path)
            {
                return Some(package_specifier);
            }

            if let Some(package_specifier) =
                self.package_specifier_for_package_json_path(current_path, source_path)
            {
                return Some(package_specifier);
            }

            if let Some(package_specifier) =
                self.package_specifier_for_file_dependency_path(current_path, source_path)
            {
                return Some(package_specifier);
            }

            let rel_path = self.calculate_relative_path(current_path, source_path);
            Some(self.strip_ts_extensions(&rel_path))
        };

        if let Some(ambient_path) = self.check_ambient_module(sym_id, binder) {
            return Some(ambient_path);
        }

        if let Some(source_arena) = binder.symbol_arenas.get(&sym_id) {
            if let Some(path) = resolve_from_arena(source_arena) {
                return Some(path);
            }
        }

        if let Some(source_arena) = self.global_symbol_arenas.get(&sym_id) {
            if let Some(path) = resolve_from_arena(source_arena) {
                return Some(path);
            }
        }

        None
    }

    #[allow(dead_code)]
    pub(crate) fn resolve_symbol_module_path_cached(&mut self, sym_id: SymbolId) -> Option<String> {
        if let Some(cached) = self.symbol_module_specifier_cache.get(&sym_id) {
            return cached.clone();
        }

        let resolved = self.resolve_symbol_module_path(sym_id);
        self.symbol_module_specifier_cache
            .insert(sym_id, resolved.clone());
        resolved
    }

    pub(in crate::declaration_emitter) fn is_namespace_import_alias_symbol(
        &self,
        sym_id: SymbolId,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };

        symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            && symbol.import_module.is_some()
            && (symbol.import_name.is_none() || symbol.import_name.as_deref() == Some("*"))
    }

    pub(crate) fn resolve_namespace_import_alias(&self, sym_id: SymbolId) -> Option<String> {
        let binder = self.binder?;

        if self.is_namespace_import_alias_symbol(sym_id) {
            return binder
                .symbols
                .get(sym_id)
                .map(|symbol| symbol.escaped_name.clone());
        }

        let module_path = self.resolve_symbol_module_path(sym_id)?;

        let mut local_imports: Vec<SymbolId> = self.import_symbol_map.keys().copied().collect();
        local_imports.sort();

        for import_sym_id in local_imports {
            let Some(symbol) = binder.symbols.get(import_sym_id) else {
                continue;
            };
            if !self.is_namespace_import_alias_symbol(import_sym_id) {
                continue;
            }
            if symbol.import_module.as_deref() == Some(module_path.as_str()) {
                return Some(symbol.escaped_name.clone());
            }
        }

        None
    }

    /// Check if a symbol is from an ambient module declaration.
    ///
    /// Returns the module name if the symbol is declared inside `declare module "name"`.
    pub(crate) fn check_ambient_module(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<String> {
        let symbol = binder.symbols.get(sym_id)?;

        // Walk up the parent chain
        let mut current_sym = symbol;
        let mut parent_id = current_sym.parent;
        while parent_id.is_some() {
            let parent_sym = binder.symbols.get(parent_id)?;

            // Check if parent is a module declaration
            if parent_sym.flags & tsz_binder::symbol_flags::MODULE != 0 {
                // Check if this module is in declared_modules
                let module_name = &parent_sym.escaped_name;
                if binder.declared_modules.contains(module_name) {
                    return Some(module_name.clone());
                }
            }

            current_sym = parent_sym;
            parent_id = current_sym.parent;
        }

        None
    }

    /// Calculate relative path from current file to source file.
    ///
    /// Returns a path like "../utils" or "./helper"
    pub(crate) fn calculate_relative_path(&self, current: &str, source: &str) -> String {
        use std::path::{Component, Path};

        let current_path = Path::new(current);
        let source_path = Path::new(source);

        // Get parent directories
        let current_dir = current_path.parent().unwrap_or(current_path);

        // Find common prefix and build relative path
        let current_components: Vec<_> = current_dir.components().collect();
        let source_components: Vec<_> = source_path.components().collect();

        // Find common prefix length
        let common_len = current_components
            .iter()
            .zip(source_components.iter())
            .take_while(|(a, b)| a == b)
            .count();

        // Build relative path: go up from current_dir, then down to source
        let ups = current_components.len() - common_len;
        let mut result = String::new();

        if ups == 0 {
            result.push_str("./");
        } else {
            for _ in 0..ups {
                result.push_str("../");
            }
        }

        // Append remaining source path components
        let remaining: Vec<_> = source_components[common_len..]
            .iter()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();
        result.push_str(&remaining.join("/"));

        // Normalize separators
        result.replace('\\', "/")
    }

    pub(in crate::declaration_emitter) fn package_specifier_for_node_modules_path(
        &self,
        current_path: &str,
        source_path: &str,
    ) -> Option<String> {
        let (source_root, source_specifier) = self.node_modules_package_info(source_path)?;
        let current_root = self
            .node_modules_package_info(current_path)
            .map(|(root, _)| root);

        if current_root.as_deref() == Some(source_root.as_str()) {
            return None;
        }

        Some(source_specifier)
    }

    pub(in crate::declaration_emitter) fn node_modules_package_info(
        &self,
        path: &str,
    ) -> Option<(String, String)> {
        use std::path::{Component, Path};

        let components: Vec<_> = Path::new(path).components().collect();
        let node_modules_idx = components.iter().rposition(|component| {
            matches!(
                component,
                Component::Normal(part) if part.to_str() == Some("node_modules")
            )
        })?;

        let trailing_parts: Vec<String> = components[node_modules_idx + 1..]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str().map(str::to_string),
                _ => None,
            })
            .collect();
        if trailing_parts.is_empty() {
            return None;
        }

        let package_len = if trailing_parts.first()?.starts_with('@') {
            2
        } else {
            1
        };
        if trailing_parts.len() < package_len {
            return None;
        }

        let package_root_components = &components[..node_modules_idx + 1 + package_len];
        let root_key = package_root_components
            .iter()
            .filter_map(|component| match component {
                Component::Prefix(prefix) => prefix.as_os_str().to_str().map(str::to_string),
                Component::RootDir => Some(String::new()),
                Component::Normal(part) => part.to_str().map(str::to_string),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        let package_root = Path::new(path)
            .components()
            .take(node_modules_idx + 1 + package_len)
            .collect::<std::path::PathBuf>();
        let package_name = trailing_parts[..package_len].join("/");
        let package_relative_parts = trailing_parts[package_len..].to_vec();
        let relative_path = package_relative_parts.join("/");
        if self.package_json_decl_entry_matches(&package_root, &relative_path) {
            return Some((root_key, package_name));
        }

        let runtime_relative_path = self.declaration_runtime_relative_path(&relative_path)?;

        let subpath = self
            .reverse_export_specifier_for_runtime_path(&package_root, &runtime_relative_path)
            .or_else(|| {
                let mut specifier_parts = package_relative_parts;
                if let Some(last) = specifier_parts.last_mut() {
                    *last = self.strip_ts_extensions(last);
                }
                if specifier_parts.last().is_some_and(|part| part == "index") {
                    specifier_parts.pop();
                }
                Some(specifier_parts.join("/"))
            })?;

        let specifier = if subpath.is_empty() {
            package_name
        } else {
            format!("{package_name}/{subpath}")
        };

        Some((root_key, specifier))
    }

    pub(in crate::declaration_emitter) fn package_specifier_for_package_json_path(
        &self,
        current_path: &str,
        source_path: &str,
    ) -> Option<String> {
        use std::path::Path;

        let source = Path::new(source_path);
        let mut package_root = source.parent()?;
        let package_json = loop {
            let candidate = package_root.join("package.json");
            if candidate.is_file() {
                break candidate;
            }
            package_root = package_root.parent()?;
        };

        let package_json_text = std::fs::read_to_string(&package_json).ok()?;
        let package_json = serde_json::from_str::<serde_json::Value>(&package_json_text).ok()?;
        let package_name = package_json.get("name")?.as_str()?;
        if package_name.is_empty() {
            return None;
        }

        let current_dir = Path::new(current_path).parent()?;
        let package_root_canonical = package_root.canonicalize().ok()?;
        let mut ancestor = Some(current_dir);
        let mut reachable = false;
        while let Some(dir) = ancestor {
            let candidate = dir.join("node_modules").join(package_name);
            if candidate.exists()
                && let Ok(candidate_canonical) = candidate.canonicalize()
                && candidate_canonical == package_root_canonical
            {
                reachable = true;
                break;
            }
            ancestor = dir.parent();
        }
        if !reachable {
            return None;
        }

        let relative = source
            .strip_prefix(package_root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        if self.package_json_decl_entry_matches(package_root, &relative) {
            return Some(package_name.to_string());
        }

        let runtime_relative_path = self.declaration_runtime_relative_path(&relative)?;
        let subpath = self
            .reverse_export_specifier_for_runtime_path(package_root, &runtime_relative_path)
            .or_else(|| {
                let mut relative_path = self.strip_ts_extensions(&relative);
                if relative_path.ends_with("/index") {
                    relative_path.truncate(relative_path.len() - "/index".len());
                } else if relative_path == "index" {
                    relative_path.clear();
                }
                Some(relative_path)
            })?;

        if subpath.is_empty() {
            Some(package_name.to_string())
        } else {
            Some(format!("{package_name}/{subpath}"))
        }
    }

    pub(in crate::declaration_emitter) fn package_specifier_for_file_dependency_path(
        &self,
        current_path: &str,
        source_path: &str,
    ) -> Option<String> {
        use std::path::Path;

        let source = Path::new(source_path);
        let source_canonical = source.canonicalize().ok();
        let mut current_dir = Path::new(current_path).parent();

        while let Some(dir) = current_dir {
            let package_json_path = dir.join("package.json");
            if let Ok(package_json_text) = std::fs::read_to_string(&package_json_path)
                && let Ok(package_json) =
                    serde_json::from_str::<serde_json::Value>(&package_json_text)
            {
                for section in [
                    "dependencies",
                    "devDependencies",
                    "peerDependencies",
                    "optionalDependencies",
                ] {
                    let Some(entries) = package_json
                        .get(section)
                        .and_then(|value| value.as_object())
                    else {
                        continue;
                    };
                    for (package_name, specifier) in entries {
                        let Some(specifier) = specifier.as_str() else {
                            continue;
                        };
                        let Some(target) = specifier
                            .strip_prefix("file:")
                            .or_else(|| specifier.strip_prefix("link:"))
                        else {
                            continue;
                        };
                        let package_root = Self::normalize_path_components(&dir.join(target));
                        let package_root_canonical = package_root.canonicalize().ok();
                        let source_is_inside_package = source_canonical
                            .as_ref()
                            .zip(package_root_canonical.as_ref())
                            .is_some_and(|(source, root)| source.starts_with(root))
                            || source.starts_with(&package_root);
                        if !source_is_inside_package {
                            continue;
                        }

                        let relative = if let Some(source_canonical) = source_canonical.as_ref()
                            && let Some(package_root_canonical) = package_root_canonical.as_ref()
                        {
                            source_canonical.strip_prefix(package_root_canonical).ok()?
                        } else {
                            source.strip_prefix(&package_root).ok()?
                        };
                        let mut relative_path = relative.to_string_lossy().replace('\\', "/");
                        relative_path = self.strip_ts_extensions(&relative_path);
                        if relative_path.ends_with("/index") {
                            relative_path.truncate(relative_path.len() - "/index".len());
                        } else if relative_path == "index" {
                            relative_path.clear();
                        }

                        return if relative_path.is_empty() {
                            Some(package_name.to_string())
                        } else {
                            Some(format!("{package_name}/{relative_path}"))
                        };
                    }
                }
            }
            current_dir = dir.parent();
        }

        None
    }

    fn normalize_path_components(path: &std::path::Path) -> std::path::PathBuf {
        let mut normalized = std::path::PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                _ => normalized.push(component.as_os_str()),
            }
        }
        normalized
    }

    fn package_json_decl_entry_matches(
        &self,
        package_root: &std::path::Path,
        relative_path: &str,
    ) -> bool {
        let package_json_path = package_root.join("package.json");
        let Ok(package_json_text) = std::fs::read_to_string(package_json_path) else {
            return false;
        };
        let Ok(package_json) = serde_json::from_str::<serde_json::Value>(&package_json_text) else {
            return false;
        };
        let Some(entry) = package_json
            .get("types")
            .or_else(|| package_json.get("typings"))
            .and_then(|value| value.as_str())
        else {
            return false;
        };

        Self::normalize_package_relative_path(entry)
            == Self::normalize_package_relative_path(relative_path)
    }

    fn normalize_package_relative_path(path: &str) -> String {
        path.replace('\\', "/")
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_string()
    }

    pub(in crate::declaration_emitter) fn declaration_runtime_relative_path(
        &self,
        relative_path: &str,
    ) -> Option<String> {
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

    pub(in crate::declaration_emitter) fn reverse_export_specifier_for_runtime_path(
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

    pub(in crate::declaration_emitter) fn reverse_match_exports_subpath(
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

    pub(in crate::declaration_emitter) fn reverse_match_export_entry(
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

    pub(in crate::declaration_emitter) fn match_export_target(
        &self,
        subpath_key: &str,
        target: &str,
        runtime_path: &str,
    ) -> Option<String> {
        let target = target.trim();
        let runtime_path = runtime_path.trim();

        if target.contains('*') {
            let wildcard = self.match_exports_wildcard(target, runtime_path)?;
            return Some(self.apply_exports_wildcard(subpath_key, &wildcard));
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

    pub(in crate::declaration_emitter) fn match_exports_wildcard(
        &self,
        pattern: &str,
        value: &str,
    ) -> Option<String> {
        let star_idx = pattern.find('*')?;
        let prefix = &pattern[..star_idx];
        let suffix = &pattern[star_idx + 1..];
        let middle = value.strip_prefix(prefix)?.strip_suffix(suffix)?;
        Some(middle.to_string())
    }

    pub(in crate::declaration_emitter) fn apply_exports_wildcard(
        &self,
        pattern: &str,
        wildcard: &str,
    ) -> String {
        pattern
            .replace('*', wildcard)
            .trim_start_matches("./")
            .to_string()
    }

    /// Strip TypeScript file extensions from a path.
    ///
    /// Converts "../utils.ts" -> "../utils"
    pub(crate) fn strip_ts_extensions(&self, path: &str) -> String {
        // Remove TypeScript and JavaScript source/declaration extensions.
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

    pub(in crate::declaration_emitter) fn normalized_source_path(
        &self,
        path: &str,
    ) -> std::path::PathBuf {
        use std::path::Component;

        std::path::Path::new(&self.strip_ts_extensions(path))
            .components()
            .filter(|component| !matches!(component, Component::CurDir))
            .collect()
    }

    pub(in crate::declaration_emitter) fn paths_refer_to_same_source_file(
        &self,
        current_path: &str,
        source_path: &str,
    ) -> bool {
        let current = self.normalized_source_path(current_path);
        let source = self.normalized_source_path(source_path);

        if current == source || current.ends_with(&source) || source.ends_with(&current) {
            return true;
        }

        let canonical_current = std::fs::canonicalize(current_path)
            .ok()
            .map(|path| self.normalized_source_path(path.to_string_lossy().as_ref()));
        let canonical_source = std::fs::canonicalize(source_path)
            .ok()
            .map(|path| self.normalized_source_path(path.to_string_lossy().as_ref()));

        canonical_current
            .zip(canonical_source)
            .is_some_and(|(a, b)| a == b)
    }

    /// Group foreign symbols by their module paths.
    ///
    /// Returns a map of module path -> Vec<SymbolId> for all foreign symbols.
    #[allow(dead_code)]
    pub(crate) fn group_foreign_symbols_by_module(&mut self) -> FxHashMap<String, Vec<SymbolId>> {
        let mut module_map: FxHashMap<String, Vec<SymbolId>> = FxHashMap::default();

        debug!(
            "[DEBUG] group_foreign_symbols_by_module: foreign_symbols = {:?}",
            self.foreign_symbols
        );

        let foreign_symbols: Vec<SymbolId> = self
            .foreign_symbols
            .as_ref()
            .map(|symbols| symbols.iter().copied().collect())
            .unwrap_or_default();

        for sym_id in foreign_symbols {
            debug!(
                "[DEBUG] group_foreign_symbols_by_module: resolving symbol {:?}",
                sym_id
            );
            if let Some(module_path) = self.resolve_symbol_module_path_cached(sym_id) {
                debug!(
                    "[DEBUG] group_foreign_symbols_by_module: symbol {:?} -> module '{}'",
                    sym_id, module_path
                );
                module_map.entry(module_path).or_default().push(sym_id);
            } else {
                debug!(
                    "[DEBUG] group_foreign_symbols_by_module: symbol {:?} -> no module path",
                    sym_id
                );
            }
        }

        debug!(
            "[DEBUG] group_foreign_symbols_by_module: returning {} modules",
            module_map.len()
        );
        module_map
    }

    pub(crate) fn prepare_import_plan(&mut self) {
        let mut plan = ImportPlan::default();

        let mut required_modules: Vec<String> = self.required_imports.keys().cloned().collect();
        required_modules.sort();
        for module in required_modules {
            let Some(symbol_names) = self.required_imports.get(&module) else {
                continue;
            };
            if symbol_names.is_empty() {
                continue;
            }

            let mut deduped = symbol_names.clone();
            deduped.sort();
            deduped.dedup();

            let symbols = deduped
                .into_iter()
                .map(|name| {
                    let alias = self
                        .import_string_aliases
                        .get(&(module.clone(), name.clone()))
                        .cloned();
                    PlannedImportSymbol { name, alias }
                })
                .collect();

            plan.required.push(PlannedImportModule { module, symbols });
        }

        // NOTE: Auto-generated imports for foreign symbols are intentionally
        // disabled. Source import declarations are now emitted faithfully
        // (preserving `type` modifiers, `with` attributes, aliases, etc.)
        // through `emit_import_declaration`, making auto-imports redundant
        // for symbols that have source imports. Symbols referenced only via
        // inline `import("pkg").Foo` type syntax don't need import
        // declarations at all. This avoids duplicate import lines that were
        // previously generated for resolution-mode imports.

        self.import_plan = plan;
    }

    pub(in crate::declaration_emitter) fn emit_import_modules(
        &mut self,
        modules: &[PlannedImportModule],
    ) {
        for module in modules {
            self.write_indent();
            self.write("import { ");

            let mut first = true;
            for symbol in &module.symbols {
                if !first {
                    self.write(", ");
                }
                first = false;

                self.write(&symbol.name);
                if let Some(alias) = &symbol.alias {
                    self.write(" as ");
                    self.write(alias);
                }
            }

            self.write(" } from \"");
            self.write(&module.module);
            self.write("\";");
            self.write_line();
        }
    }

    /// Emit auto-generated imports for foreign symbols.
    ///
    /// This should be called before emitting other declarations to ensure
    /// imports appear at the top of the .d.ts file.
    pub(crate) fn emit_auto_imports(&mut self) {
        let modules = std::mem::take(&mut self.import_plan.auto_generated);
        self.emit_import_modules(&modules);
        self.import_plan.auto_generated = modules;
    }
}
