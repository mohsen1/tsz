fn resolve_lib_reference_path(base_path: &Path, lib_name: &str) -> Option<PathBuf> {
    let lib_dir = base_path.parent()?;
    let normalized = normalize_lib_reference_name(lib_name);
    let mut candidate_names = vec![normalized.clone()];
    match normalized.as_str() {
        // Source-tree libs use *.generated.d.ts while built/local and npm libs use plain names.
        "dom" => candidate_names.push("dom.generated".to_string()),
        "dom.iterable" => candidate_names.push("dom.iterable.generated".to_string()),
        "dom.asynciterable" => candidate_names.push("dom.asynciterable.generated".to_string()),
        "dom.generated" => candidate_names.push("dom".to_string()),
        "dom.iterable.generated" => candidate_names.push("dom.iterable".to_string()),
        "dom.asynciterable.generated" => candidate_names.push("dom.asynciterable".to_string()),
        _ => {}
    }
    if base_path.starts_with(Path::new("/embedded-lib")) {
        return candidate_names.into_iter().find_map(|name| {
            let embedded_name = format!("{name}.d.ts");
            crate::embedded_libs::is_embedded_lib(&embedded_name)
                .then(|| lib_dir.join(embedded_name))
        });
    }
    let candidates: Vec<PathBuf> = candidate_names
        .into_iter()
        .flat_map(|name| {
            [
                lib_dir.join(format!("lib.{name}.d.ts")),
                lib_dir.join(format!("{name}.d.ts")),
            ]
        })
        .collect();
    // Check embedded libs first (no syscall), then fall back to disk stat.
    candidates.into_iter().find(|candidate| {
        candidate
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(crate::embedded_libs::is_embedded_lib)
            || candidate.exists()
    })
}

fn normalize_lib_reference_name(name: &str) -> String {
    match name.to_lowercase().trim() {
        "es6" => "es6".to_string(),
        "es7" => "es2016".to_string(),
        "lib" | "lib.d.ts" => "es5".to_string(),
        // Modern TypeScript (6.x) uses lib.dom.d.ts directly, not .generated suffix.
        // Pass through as-is — the file candidates already include lib.{name}.d.ts.
        "dom" | "dom.iterable" | "dom.asynciterable" => name.to_lowercase(),
        s if s.starts_with("lib.") && s.ends_with(".d.ts") => {
            let inner = &s[4..s.len() - 5];
            normalize_lib_reference_name(inner)
        }
        other => other.to_string(),
    }
}

/// Parse and bind multiple files in parallel with lib symbol injection.
///
/// This is the main entry point for compilation that includes lib.d.ts symbols.
/// Lib files are loaded first, then each file is parsed and bound with lib symbols
/// merged into its binder.
///
/// # Arguments
/// * `files` - Vector of (`file_name`, `source_text`) pairs
/// * `lib_files` - Optional list of lib file paths to load
///
/// # Returns
/// Vector of `BindResult` for each file
pub fn parse_and_bind_parallel_with_lib_files(
    files: Vec<(String, String)>,
    lib_files: &[&Path],
) -> Vec<BindResult> {
    // Load lib files for binding.
    // This path is intentionally strict so missing/unreadable lib files are not ignored.
    let lib_contexts = load_lib_files_for_binding_strict(lib_files)
        .unwrap_or_else(|err| panic!("failed to load lib files from disk: {err}"));

    // Parse and bind with lib symbols
    parse_and_bind_parallel_with_libs(files, &lib_contexts)
}

/// Parse and bind multiple files in parallel with lib contexts.
///
/// Lib symbols are injected into each file's binder during binding,
/// enabling resolution of global symbols like `console`, `Array`, etc.
///
/// # Arguments
/// * `files` - Vector of (`file_name`, `source_text`) pairs
/// * `lib_files` - Lib files to merge into each binder
///
/// # Returns
/// Vector of `BindResult` for each file
pub fn parse_and_bind_parallel_with_libs(
    files: Vec<(String, String)>,
    lib_files: &[Arc<lib_loader::LibFile>],
) -> Vec<BindResult> {
    parse_and_bind_parallel_with_libs_and_target(files, lib_files, ScriptTarget::default())
}

/// Parse and bind multiple files in parallel with lib contexts and a compiler target.
pub fn parse_and_bind_parallel_with_libs_and_target(
    files: Vec<(String, String)>,
    lib_files: &[Arc<lib_loader::LibFile>],
    language_version: ScriptTarget,
) -> Vec<BindResult> {
    let premerged_lib_binder = if files.len() > 1 && !lib_files.is_empty() {
        let mut binder = BinderState::new();
        binder.merge_lib_symbols(lib_files);
        Some(Arc::new(binder))
    } else {
        None
    };

    if files.len() <= 1 {
        return files
            .into_iter()
            .map(|(file_name, source_text)| {
                bind_file_with_libs_with_language_version(
                    file_name,
                    source_text,
                    lib_files,
                    language_version,
                    premerged_lib_binder.as_deref(),
                )
            })
            .collect();
    }

    #[cfg(not(target_arch = "wasm32"))]
    ensure_rayon_global_pool();

    maybe_parallel_into!(files)
        .map(|(file_name, source_text)| {
            bind_file_with_libs_with_language_version(
                file_name,
                source_text,
                lib_files,
                language_version,
                premerged_lib_binder.as_deref(),
            )
        })
        .collect()
}

fn bind_file_with_libs_with_language_version(
    file_name: String,
    source_text: String,
    lib_files: &[Arc<lib_loader::LibFile>],
    language_version: ScriptTarget,
    premerged_lib_binder: Option<&BinderState>,
) -> BindResult {
    // Skip parsing .json files - they should not be parsed as TypeScript.
    // JSON module imports should be resolved during module resolution and
    // emit TS2732 if resolveJsonModule is false.
    if file_name.ends_with(".json") {
        return synthesize_json_bind_result(file_name, source_text);
    }

    // Parse
    let mut parser =
        ParserState::new_with_language_version(file_name.clone(), source_text, language_version);
    let source_file = parser.parse_source_file();

    let (arena, parse_diagnostics) = parser.into_parts();

    // Bind with lib symbols
    let mut binder = premerged_lib_binder
        .cloned()
        .unwrap_or_else(BinderState::new);
    binder.set_debug_file(&file_name);

    // IMPORTANT: Merge lib symbols BEFORE binding source file
    // so that symbols like console, Array, Promise are available during binding
    if premerged_lib_binder.is_none() && !lib_files.is_empty() {
        binder.merge_lib_symbols(lib_files);
    }

    binder.bind_source_file(&arena, source_file);
    compact_premerged_lib_state(&mut binder);

    // Extract lib_binders and lib_arenas from binder before it's moved
    let lib_binders = binder.lib_binders.clone();
    let lib_arenas: Vec<Arc<NodeArena>> =
        lib_files.iter().map(|lf| Arc::clone(&lf.arena)).collect();

    BindResult {
        file_name,
        source_file,
        arena: Arc::new(arena),
        symbols: binder.symbols,
        file_locals: binder.file_locals,
        declared_modules: binder.declared_modules,
        module_exports: binder.module_exports,
        node_symbols: binder.node_symbols,
        module_declaration_exports_publicly: binder.module_declaration_exports_publicly,
        symbol_arenas: binder.symbol_arenas,
        declaration_arenas: binder.declaration_arenas,
        scopes: binder.scopes,
        node_scope_ids: binder.node_scope_ids,
        parse_diagnostics,
        shorthand_ambient_modules: binder.shorthand_ambient_modules,
        global_augmentations: binder.global_augmentations,
        module_augmentations: binder.module_augmentations,
        augmentation_target_modules: binder.augmentation_target_modules,
        reexports: binder.reexports,
        wildcard_reexports: binder.wildcard_reexports,
        lib_binders,
        lib_arenas,
        lib_symbol_ids: binder.lib_symbol_ids,
        lib_symbol_reverse_remap: binder.lib_symbol_reverse_remap,
        flow_nodes: binder.flow_nodes,
        node_flow: binder.node_flow,
        switch_clause_to_switch: std::mem::take(&mut binder.switch_clause_to_switch),
        is_external_module: binder.is_external_module,
        expando_properties: std::mem::take(&mut binder.expando_properties),
        alias_partners: binder.alias_partners,
        file_features: binder.file_features,
        semantic_defs: binder.semantic_defs,
        file_import_sources: binder.file_import_sources,
    }
}

fn compact_premerged_lib_state(binder: &mut BinderState) {
    if binder.lib_symbol_ids.is_empty() {
        return;
    }

    let lib_symbol_ids = Arc::clone(&binder.lib_symbol_ids);
    let mut retained_lib_symbols = FxHashSet::default();
    for &sym_id in binder.node_symbols.values() {
        if lib_symbol_ids.contains(&sym_id) {
            retained_lib_symbols.insert(sym_id);
        }
    }

    collect_retained_lib_symbol_refs(binder, &lib_symbol_ids, &mut retained_lib_symbols);

    binder.file_locals =
        strip_pure_lib_entries(&binder.file_locals, &lib_symbol_ids, &retained_lib_symbols);

    for scope in Arc::make_mut(&mut binder.scopes) {
        scope.table = strip_pure_lib_entries(&scope.table, &lib_symbol_ids, &retained_lib_symbols);
    }

    let id_remap = densify_bind_symbols(binder, &lib_symbol_ids, &retained_lib_symbols);
    remap_compacted_bind_state(binder, &id_remap);
}

fn strip_pure_lib_entries(
    table: &SymbolTable,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained_lib_symbols: &FxHashSet<SymbolId>,
) -> SymbolTable {
    let retained = table
        .iter()
        .filter(|(_, sym_id)| {
            !lib_symbol_ids.contains(sym_id) || retained_lib_symbols.contains(sym_id)
        })
        .count();
    let mut stripped = SymbolTable::with_capacity(retained);
    for (name, &sym_id) in table.iter() {
        if !lib_symbol_ids.contains(&sym_id) || retained_lib_symbols.contains(&sym_id) {
            stripped.set(name.clone(), sym_id);
        }
    }
    stripped
}

fn collect_retained_lib_symbol_refs(
    binder: &BinderState,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained: &mut FxHashSet<SymbolId>,
) {
    for table in binder.module_exports.values() {
        collect_lib_ids_from_table(table, lib_symbol_ids, retained);
    }
    for (&key, value) in binder.alias_partners.iter() {
        retain_if_lib(key, lib_symbol_ids, retained);
        retain_if_lib(*value, lib_symbol_ids, retained);
    }
    for &sym_id in binder.augmentation_target_modules.keys() {
        retain_if_lib(sym_id, lib_symbol_ids, retained);
    }

    for sym in binder.symbols.iter() {
        if lib_symbol_ids.contains(&sym.id) {
            continue;
        }
        retain_if_lib(sym.parent, lib_symbol_ids, retained);
        if let Some(exports) = sym.exports.as_ref() {
            collect_lib_ids_from_table(exports, lib_symbol_ids, retained);
        }
        if let Some(members) = sym.members.as_ref() {
            collect_lib_ids_from_table(members, lib_symbol_ids, retained);
        }
    }

    for scope in binder.scopes.iter() {
        if scope.kind != crate::binder::ContainerKind::SourceFile {
            collect_lib_ids_from_table(&scope.table, lib_symbol_ids, retained);
        }
    }
}

fn collect_lib_ids_from_table(
    table: &SymbolTable,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained: &mut FxHashSet<SymbolId>,
) {
    for (_, &sym_id) in table.iter() {
        retain_if_lib(sym_id, lib_symbol_ids, retained);
    }
}

fn retain_if_lib(
    sym_id: SymbolId,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained: &mut FxHashSet<SymbolId>,
) {
    if lib_symbol_ids.contains(&sym_id) {
        retained.insert(sym_id);
    }
}

fn densify_bind_symbols(
    binder: &mut BinderState,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained_lib_symbols: &FxHashSet<SymbolId>,
) -> FxHashMap<SymbolId, SymbolId> {
    let retained_count = binder
        .symbols
        .iter()
        .filter(|sym| !lib_symbol_ids.contains(&sym.id) || retained_lib_symbols.contains(&sym.id))
        .count();
    let mut compacted_symbols = SymbolArena::with_capacity(retained_count);
    let mut id_remap = FxHashMap::with_capacity_and_hasher(retained_count, Default::default());

    for sym in binder.symbols.iter() {
        if lib_symbol_ids.contains(&sym.id) && !retained_lib_symbols.contains(&sym.id) {
            continue;
        }
        let old_id = sym.id;
        let new_id = compacted_symbols.alloc_from(sym);
        id_remap.insert(old_id, new_id);
    }

    for sym in compacted_symbols.iter_mut() {
        sym.parent = id_remap.get(&sym.parent).copied().unwrap_or(SymbolId::NONE);
        if let Some(exports) = sym.exports.as_ref() {
            sym.exports = remap_symbol_table_option(exports, &id_remap).map(Box::new);
        }
        if let Some(members) = sym.members.as_ref() {
            sym.members = remap_symbol_table_option(members, &id_remap).map(Box::new);
        }
    }

    binder.symbols = compacted_symbols;
    id_remap
}

fn remap_compacted_bind_state(binder: &mut BinderState, id_remap: &FxHashMap<SymbolId, SymbolId>) {
    binder.file_locals = remap_symbol_table_required(&binder.file_locals, id_remap);

    for scope in Arc::make_mut(&mut binder.scopes) {
        scope.table = remap_symbol_table_required(&scope.table, id_remap);
    }

    binder.node_symbols = Arc::new(
        binder
            .node_symbols
            .iter()
            .filter_map(|(&node, sym_id)| {
                id_remap.get(sym_id).copied().map(|new_id| (node, new_id))
            })
            .collect(),
    );

    binder.module_exports = Arc::new(
        binder
            .module_exports
            .iter()
            .filter_map(|(key, table)| {
                remap_symbol_table_option(table, id_remap).map(|remapped| (key.clone(), remapped))
            })
            .collect(),
    );

    binder.symbol_arenas = Arc::new(
        binder
            .symbol_arenas
            .iter()
            .filter_map(|(sym_id, arena)| {
                id_remap
                    .get(sym_id)
                    .copied()
                    .map(|new_id| (new_id, Arc::clone(arena)))
            })
            .collect(),
    );

    binder.declaration_arenas = Arc::new(
        binder
            .declaration_arenas
            .iter()
            .filter_map(|(&(sym_id, decl_idx), arenas)| {
                id_remap
                    .get(&sym_id)
                    .copied()
                    .map(|new_id| ((new_id, decl_idx), arenas.clone()))
            })
            .collect(),
    );

    binder.augmentation_target_modules = Arc::new(
        binder
            .augmentation_target_modules
            .iter()
            .filter_map(|(sym_id, target)| {
                id_remap
                    .get(sym_id)
                    .copied()
                    .map(|new_id| (new_id, target.clone()))
            })
            .collect(),
    );

    binder.lib_symbol_ids = Arc::new(
        binder
            .lib_symbol_ids
            .iter()
            .filter_map(|sym_id| id_remap.get(sym_id).copied())
            .collect(),
    );
    binder.lib_symbol_reverse_remap = Arc::new(
        binder
            .lib_symbol_reverse_remap
            .iter()
            .filter_map(|(sym_id, target)| {
                id_remap
                    .get(sym_id)
                    .copied()
                    .map(|new_id| (new_id, *target))
            })
            .collect(),
    );

    binder.alias_partners = Arc::new(
        binder
            .alias_partners
            .iter()
            .filter_map(|(left, right)| {
                let new_left = id_remap.get(left).copied()?;
                let new_right = id_remap.get(right).copied()?;
                Some((new_left, new_right))
            })
            .collect(),
    );

    binder.semantic_defs = Arc::new(
        binder
            .semantic_defs
            .iter()
            .filter_map(|(sym_id, entry)| {
                id_remap
                    .get(sym_id)
                    .copied()
                    .map(|new_id| (new_id, remap_semantic_def_entry(entry, id_remap)))
            })
            .collect(),
    );

    binder.expando_properties = remap_expando_properties(&binder.expando_properties, id_remap);
    // All SymbolIds were remapped; any cached (name → old_id) results are now stale.
    binder.clear_resolution_caches();
}
