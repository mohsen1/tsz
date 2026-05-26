use super::*;

pub(super) struct SourceResolutionSetup {
    pub(super) cached_module_specifiers: Vec<Vec<CachedModuleSpecifier>>,
    pub(super) resolved_module_paths: Arc<FxHashMap<(usize, String), usize>>,
    pub(super) resolved_module_request_paths: Arc<FxHashMap<ResolutionRequestMapKey, usize>>,
    pub(super) resolved_module_ts_extension_flags: Arc<FxHashMap<(usize, String), bool>>,
    pub(super) resolved_module_errors:
        Arc<FxHashMap<(usize, String), tsz::checker::context::ResolutionError>>,
    pub(super) resolved_module_request_errors:
        Arc<FxHashMap<ResolutionRequestMapKey, tsz::checker::context::ResolutionError>>,
    pub(super) resolved_modules_per_file: Arc<Vec<Arc<rustc_hash::FxHashSet<String>>>>,
}

pub(super) struct SourceResolutionSetupInput<'a> {
    pub(super) program: &'a MergedProgram,
    pub(super) options: &'a ResolvedCompilerOptions,
    pub(super) base_dir: &'a Path,
    pub(super) source_module_resolutions:
        Option<&'a FxHashMap<SourceModuleResolutionKey, SourceModuleResolution>>,
    pub(super) canonical_to_file_idx: &'a FxHashMap<PathBuf, usize>,
    pub(super) program_paths: &'a FxHashSet<PathBuf>,
    pub(super) package_redirects: &'a FxHashMap<PathBuf, PathBuf>,
    pub(super) resolution_cache: &'a mut ModuleResolutionCache,
}

pub(super) fn prepare_source_resolution_setup(
    input: SourceResolutionSetupInput<'_>,
) -> SourceResolutionSetup {
    let SourceResolutionSetupInput {
        program,
        options,
        base_dir,
        source_module_resolutions,
        canonical_to_file_idx,
        program_paths,
        package_redirects,
        resolution_cache,
    } = input;

    // Cache module specifiers per file — collected once, reused in prepare_binders
    // and check_file_for_parallel to avoid 3× redundant AST traversals.
    let cached_module_specifiers: Vec<Vec<CachedModuleSpecifier>> = {
        let _span =
            tracing::info_span!("collect_module_specifiers", files = program.files.len()).entered();
        if program.files.len() <= FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES {
            program
                .files
                .iter()
                .map(|file| {
                    collect_module_specifiers_for_check(
                        &file.arena,
                        file.source_file,
                        file.is_external_module,
                    )
                })
                .collect()
        } else {
            use rayon::prelude::*;
            program
                .files
                .par_iter()
                .map(|file| {
                    collect_module_specifiers_for_check(
                        &file.arena,
                        file.source_file,
                        file.is_external_module,
                    )
                })
                .collect()
        }
    };

    // Create ModuleResolver instance for proper error reporting (TS2834, TS2835, TS2792, etc.)
    let mut module_resolver = ModuleResolver::new(options);
    let module_specifier_count: usize = cached_module_specifiers.iter().map(Vec::len).sum();

    // Build resolved_module_paths map: (source_file_idx, specifier) -> target_file_idx
    // Also build resolved_module_errors map for specific error codes.
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> =
        FxHashMap::with_capacity_and_hasher(module_specifier_count, Default::default());
    // Per-resolution `resolvedUsingTsExtension` flag — populated when the
    // resolver consumed a `.ts` extension via a literal package.json
    // exports/imports key. Consumed by the checker's TS2877 gate. This and the
    // error maps stay sparse: most programs resolve without these entries.
    let mut resolved_module_ts_extension_flags: FxHashMap<(usize, String), bool> =
        FxHashMap::default();
    let mut resolved_module_request_paths: FxHashMap<ResolutionRequestMapKey, usize> =
        FxHashMap::with_capacity_and_hasher(module_specifier_count, Default::default());
    let mut resolved_module_specifiers: FxHashSet<(usize, String)> =
        FxHashSet::with_capacity_and_hasher(module_specifier_count, Default::default());
    let mut resolved_module_errors: FxHashMap<
        (usize, String),
        tsz::checker::context::ResolutionError,
    > = FxHashMap::default();
    let mut resolved_module_request_errors: FxHashMap<
        ResolutionRequestMapKey,
        tsz::checker::context::ResolutionError,
    > = FxHashMap::default();
    // Phase 2 step 1: route the module-resolver's ambient-module check through
    // `SkeletonIndex` when present. The skeleton already captured both
    // `declared_modules` and `shorthand_ambient_modules` during the parallel
    // bind phase (see `crates/tsz-core/src/parallel/skeleton.rs`), so this
    // consumer no longer needs `MergedProgram.{declared,shorthand_ambient}_modules`
    // to answer the lookup. The legacy fields remain as a fallback for the
    // small-project / sequential path where no skeleton is computed.
    //
    // This is consumer-side only: `MergedProgram` retains both fields unchanged.
    let skeleton_for_ambient: Option<&tsz::parallel::SkeletonIndex> =
        program.skeleton_index.as_ref();
    let wildcard_ambient_modules_for_resolution = source_module_resolutions.and_then(|_| {
        let has_wildcard_ambient = program
            .declared_modules
            .iter()
            .chain(program.shorthand_ambient_modules.iter())
            .any(|name| name.contains('*'));
        has_wildcard_ambient.then(|| {
            tsz::checker::context::GlobalDeclaredModules::from_module_names(
                program
                    .declared_modules
                    .iter()
                    .chain(program.shorthand_ambient_modules.iter()),
            )
        })
    });
    {
        let _span = tracing::info_span!("build_resolved_module_maps").entered();
        for (file_idx, file) in program.files.iter().enumerate() {
            let file_path = Path::new(&file.file_name);

            for (specifier, specifier_node, import_kind, resolution_mode_override) in
                &cached_module_specifiers[file_idx]
            {
                let span = if let Some(spec_node) = file.arena.get(*specifier_node) {
                    Span::new(spec_node.pos, spec_node.end)
                } else {
                    Span::new(0, 0)
                };

                let request = tsz::module_resolver::ModuleLookupRequest {
                    specifier,
                    containing_file: file_path,
                    specifier_span: span,
                    import_kind: *import_kind,
                    resolution_mode_override: *resolution_mode_override,
                    no_implicit_any: options.checker.no_implicit_any,
                    implied_classic_resolution: options.checker.implied_classic_resolution,
                };
                let request_mode_key = checker_lookup_resolution_mode(
                    &mut module_resolver,
                    options,
                    file_path,
                    *import_kind,
                    *resolution_mode_override,
                );
                let request_kind_key = checker_resolution_request_kind(*import_kind);
                if let Some(discovered) = source_module_resolutions.and_then(|resolutions| {
                    resolutions.get(&SourceModuleResolutionKey {
                        containing_file: file_path.to_path_buf(),
                        specifier: specifier.clone(),
                        import_kind: *import_kind,
                        resolution_mode_override: *resolution_mode_override,
                    })
                }) {
                    resolved_module_specifiers.insert((file_idx, specifier.clone()));
                    let canonical = if should_apply_duplicate_package_redirect(file_path) {
                        package_redirects
                            .get(&discovered.canonical_path)
                            .cloned()
                            .unwrap_or_else(|| discovered.canonical_path.clone())
                    } else {
                        discovered.canonical_path.clone()
                    };
                    if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                        resolved_module_paths.insert((file_idx, specifier.clone()), target_idx);
                        resolved_module_request_paths.insert(
                            (
                                file_idx,
                                specifier.clone(),
                                request_mode_key,
                                request_kind_key,
                            ),
                            target_idx,
                        );
                        if discovered.resolved_using_ts_extension {
                            resolved_module_ts_extension_flags
                                .insert((file_idx, specifier.clone()), true);
                        }
                    }
                    continue;
                }

                // Source discovery has already tried to map this specifier to a
                // source/declaration file. If it failed and a program-wide
                // ambient wildcard (for example `*.svg`) covers the specifier,
                // treat it as ambient without repeating the filesystem probe.
                if wildcard_ambient_modules_for_resolution
                    .as_ref()
                    .is_some_and(|modules| modules.matches_wildcard(specifier))
                {
                    resolved_module_specifiers.insert((file_idx, specifier.clone()));
                    continue;
                }

                let result = module_resolver.lookup(
                    &request,
                    |spec, fp| {
                        resolve_module_specifier(
                            fp,
                            spec,
                            options,
                            base_dir,
                            resolution_cache,
                            program_paths,
                        )
                    },
                    |spec| {
                        // Skeleton-first: served entirely from skeleton data when present.
                        if let Some(idx) = skeleton_for_ambient {
                            return idx.is_ambient_module(spec);
                        }
                        // Fallback: legacy MergedProgram fields (no skeleton case).
                        program.declared_modules.contains(spec)
                            || program.shorthand_ambient_modules.contains(spec)
                    },
                    Some(program_paths),
                );

                // Classify the lookup result into a driver-facing outcome.
                let mut outcome = result.classify();
                if outcome
                    .error
                    .as_ref()
                    .is_some_and(|error| error.code == 2732)
                    && module_specifier_has_type_json_import_attribute(&file.arena, *specifier_node)
                    && json_type_attribute_enables_json_module(
                        options,
                        file_path,
                        base_dir,
                        resolution_cache,
                    )
                    && let Some(resolved_path) = resolve_module_specifier(
                        file_path,
                        specifier,
                        options,
                        base_dir,
                        resolution_cache,
                        program_paths,
                    )
                    && resolved_path.extension().is_some_and(|ext| ext == "json")
                {
                    outcome.resolved_path = Some(resolved_path);
                    outcome.is_resolved = true;
                    outcome.error = None;
                }

                if std::env::var_os("TSZ_DEBUG_RESOLVE").is_some() {
                    tracing::debug!(
                        "module lookup: file={} spec={} resolved={:?} is_resolved={} error={:?}",
                        file_path.display(),
                        specifier,
                        outcome.resolved_path,
                        outcome.is_resolved,
                        outcome.error,
                    );
                }

                // Map resolved path to file index.
                // Only mark as resolved when there is no error. When there is a
                // resolution error (TS2307, TS6263, etc.) the module should NOT
                // be in resolved_module_specifiers so that the checker emits the
                // appropriate diagnostic without triggering additional member checks.
                if outcome.error.is_none() {
                    if let Some(ref resolved_path) = outcome.resolved_path {
                        resolved_module_specifiers.insert((file_idx, specifier.clone()));
                        let canonical = normalize_resolved_path(resolved_path, options);
                        // Apply duplicate package redirect
                        let canonical = if should_apply_duplicate_package_redirect(file_path) {
                            package_redirects
                                .get(&canonical)
                                .cloned()
                                .unwrap_or(canonical)
                        } else {
                            canonical
                        };
                        if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                            resolved_module_paths.insert((file_idx, specifier.clone()), target_idx);
                            resolved_module_request_paths.insert(
                                (
                                    file_idx,
                                    specifier.clone(),
                                    request_mode_key,
                                    request_kind_key,
                                ),
                                target_idx,
                            );
                            if outcome.resolved_using_ts_extension {
                                resolved_module_ts_extension_flags
                                    .insert((file_idx, specifier.clone()), true);
                            }
                        }
                    } else if outcome.is_resolved {
                        resolved_module_specifiers.insert((file_idx, specifier.clone()));
                    }
                }

                // Record error for the checker
                if let Some(ref error) = outcome.error {
                    resolved_module_errors.insert(
                        (file_idx, specifier.clone()),
                        tsz::checker::context::ResolutionError {
                            code: error.code,
                            message: error.message.clone(),
                        },
                    );
                    resolved_module_request_errors.insert(
                        (
                            file_idx,
                            specifier.clone(),
                            request_mode_key,
                            request_kind_key,
                        ),
                        tsz::checker::context::ResolutionError {
                            code: error.code,
                            message: error.message.clone(),
                        },
                    );
                }
            }
        }
    }

    // Pre-bucket resolved-module specifiers by file_idx so each per-file
    // checker can look up its own set in O(1) instead of scanning the
    // entire cross-file `resolved_module_specifiers` map. The previous
    // pattern was `iter().filter(|(idx, _)| *idx == file_idx)` per file —
    // O(N_total_specifiers) per file → O(N_files × N_total_specifiers)
    // overall. On a 6086-file fixture with avg 20 imports per file
    // (~120 K total entries) that ballooned into ~700 M hashset
    // iterations across all checkers; the per-file checker scaled with
    // the size of the WHOLE program rather than its own import count.
    // Per-file `Arc<FxHashSet<String>>` so the per-file checker can share
    // the bucketed set via `Arc::clone` into `ctx.resolved_modules` without
    // a deep copy of the contents. On 6086 files × avg 20 specifiers this
    // avoids ~120K `String` clones + hashset insertions at the per-file
    // `check_file_for_parallel` entry. Build the owned buckets first, then
    // wrap each in `Arc::new` in one pass.
    let resolved_modules_per_file: Arc<Vec<Arc<rustc_hash::FxHashSet<String>>>> = Arc::new({
        let _span = tracing::info_span!(
            "bucket_resolved_modules_per_file",
            files = program.files.len()
        )
        .entered();
        let mut by_file: Vec<rustc_hash::FxHashSet<String>> = (0..program.files.len())
            .map(|_| FxHashSet::default())
            .collect();
        for (file_idx, specifier) in resolved_module_specifiers.iter() {
            if let Some(set) = by_file.get_mut(*file_idx) {
                set.insert(specifier.clone());
            }
        }
        by_file.into_iter().map(Arc::new).collect()
    });

    SourceResolutionSetup {
        cached_module_specifiers,
        resolved_module_paths: Arc::new(resolved_module_paths),
        resolved_module_request_paths: Arc::new(resolved_module_request_paths),
        resolved_module_ts_extension_flags: Arc::new(resolved_module_ts_extension_flags),
        resolved_module_errors: Arc::new(resolved_module_errors),
        resolved_module_request_errors: Arc::new(resolved_module_request_errors),
        resolved_modules_per_file,
    }
}
