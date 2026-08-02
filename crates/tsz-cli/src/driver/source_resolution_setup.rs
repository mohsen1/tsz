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
    /// (`source_file_idx`, specifier) -> absolute path of the JavaScript file a
    /// specifier resolved to when the resolution carried no declaration file.
    ///
    /// Recorded independently of `resolved_module_errors`: TS7016 is only
    /// produced under `noImplicitAny`, but the augmentation-site TS2665 check
    /// applies regardless, so it cannot be derived from the error map.
    pub(super) untyped_module_paths: Arc<FxHashMap<(usize, String), String>>,
    pub(super) resolved_modules_per_file: Arc<Vec<Arc<rustc_hash::FxHashSet<String>>>>,
    /// Pre-computed per-file TS7016 diagnostics for CJS `require()` calls.
    ///
    /// The driver's resolution pass detects untyped JS modules (TS7016), but the
    /// checker's module-not-found path skips them because the module DID resolve.
    /// For CJS `require()` calls (not import declarations), diagnostics are pre-
    /// computed here from the resolved-error maps and emitted directly.
    pub(super) per_file_ts7016_diagnostics: Arc<Vec<Vec<Diagnostic>>>,
    /// Pre-computed per-file ESM/CJS module kind for resolution modes that honor
    /// `package.json` `"type"` semantics. The checker uses this shared map for
    /// ESM-vs-CJS-sensitive diagnostics such as TS1479 and TS1192 suppression.
    pub(super) file_is_esm_map: Arc<FxHashMap<String, bool>>,
}

pub(super) struct SourceResolutionSetupInput<'a> {
    pub(super) program: &'a MergedProgram,
    pub(super) options: &'a ResolvedCompilerOptions,
    pub(super) base_dir: &'a Path,
    pub(super) source_module_resolutions:
        Option<&'a FxHashMap<SourceModuleResolutionKey, SourceModuleResolution>>,
    pub(super) source_module_resolution_misses: Option<&'a FxHashSet<SourceModuleResolutionKey>>,
    pub(super) program_file_index: &'a ProgramFileIndex,
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
        source_module_resolution_misses,
        program_file_index,
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
            // First global-pool use on large projects: without this, rayon
            // builds the pool with platform-default worker stacks and deep
            // checker recursion later overflows in a worker (#15962).
            tsz::parallel::ensure_rayon_global_pool();
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
    let mut untyped_module_paths: FxHashMap<(usize, String), String> = FxHashMap::default();
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
                    file_path,
                    *import_kind,
                    *resolution_mode_override,
                );
                let request_kind_key = checker_resolution_request_kind(*import_kind);
                let source_resolution_key = SourceModuleResolutionKey {
                    containing_file: file_path.to_path_buf(),
                    specifier: specifier.clone(),
                    import_kind: *import_kind,
                    resolution_mode_override: *resolution_mode_override,
                };
                // An exact-name `declare module "<spec>"` takes precedence over a
                // file/path resolution that points at a *different* file than the
                // one declaring the ambient module. tsc's `resolveExternalModule`
                // consults `tryFindAmbientModule` (an exact-name ambient lookup)
                // *before* `getResolvedModule`, so a project that declares
                // `declare module "graphql-scalars"` in one file and also maps the
                // same bare name through a catch-all `paths` entry (or unrelated
                // on-disk stub) must read the ambient module's named exports, not
                // the path-mapped file's surface. Source discovery runs before
                // ambient modules are known (its `is_ambient_module` closure is
                // hardcoded to `false`), so the recorded resolution can point at
                // the path-mapped stub; re-assert ambient precedence here.
                //
                // Crucially, this must NOT fire when the path-resolved file *is*
                // the file that declares the ambient module (e.g. a real package
                // whose `index.d.ts` contains `declare module "express" { ... }`
                // discovered through node resolution, possibly merged with a
                // module augmentation). In that case the ambient declaration and
                // the resolved file are the same module, so the normal resolved
                // path must stand to preserve the full module surface and any
                // augmentation merge.
                //
                // Only non-relative bare specifiers are eligible, mirroring tsc's
                // `isExternalModuleNameRelative` guard. Pattern ambient modules
                // (`*.svg`) never match a concrete specifier via the exact-name
                // membership test and keep flowing through the wildcard path.
                let specifier_is_ordinary_bare = !specifier.starts_with('.')
                    && !specifier.starts_with('/')
                    && (!specifier.contains(':') || specifier.starts_with("node:"));
                let exact_name_ambient_match = specifier_is_ordinary_bare && {
                    if let Some(idx) = skeleton_for_ambient {
                        idx.is_ambient_module(specifier)
                    } else {
                        program.declared_modules.contains(specifier.as_str())
                            || program
                                .shorthand_ambient_modules
                                .contains(specifier.as_str())
                    }
                };

                let discovered = source_module_resolutions
                    .and_then(|resolutions| resolutions.get(&source_resolution_key));

                // Resolve the discovered target to a program file index up front so
                // ambient precedence can compare it against the ambient module's
                // declaring file(s).
                let discovered_target = discovered.and_then(|discovered| {
                    let canonical = if should_apply_duplicate_package_redirect(file_path) {
                        package_redirects
                            .get(&discovered.canonical_path)
                            .cloned()
                            .unwrap_or_else(|| discovered.canonical_path.clone())
                    } else {
                        discovered.canonical_path.clone()
                    };
                    program_file_index
                        .get_with_symlink_fallback(&canonical, &canonical, options)
                        .map(|target_idx| (target_idx, discovered.resolved_using_ts_extension))
                });

                // The ambient module's declaring file(s): files whose binder
                // recorded a bodied `declare module "<spec>"` export surface.
                // When the discovered resolution target is one of these, the
                // ambient is the resolved file itself — keep the resolved path.
                let discovered_target_declares_ambient = exact_name_ambient_match
                    && match (skeleton_for_ambient, discovered_target) {
                        (Some(idx), Some((target_idx, _))) => {
                            idx.module_binders_for(specifier).contains(&target_idx)
                        }
                        // No skeleton: fall back to keeping the resolved path
                        // whenever a concrete file resolved (conservative — only
                        // path-less ambient when nothing resolved on disk). This
                        // avoids dropping a genuine same-file ambient/package in
                        // the sequential path where per-file declarer data is not
                        // projected.
                        (None, Some(_)) => true,
                        _ => false,
                    };

                if exact_name_ambient_match && !discovered_target_declares_ambient {
                    resolved_module_specifiers.insert((file_idx, specifier.clone()));
                    continue;
                }

                if let Some((target_idx, resolved_using_ts_extension)) = discovered_target {
                    resolved_module_specifiers.insert((file_idx, specifier.clone()));
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
                    if resolved_using_ts_extension {
                        resolved_module_ts_extension_flags
                            .insert((file_idx, specifier.clone()), true);
                    }
                    continue;
                }
                if discovered.is_some() {
                    // Discovered a resolution but the target is not in the program
                    // file index (e.g. filtered out). Preserve the prior behavior
                    // of marking the specifier resolved without a path.
                    resolved_module_specifiers.insert((file_idx, specifier.clone()));
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

                let source_discovery_missed = source_module_resolution_misses
                    .is_some_and(|misses| misses.contains(&source_resolution_key));
                let result = module_resolver.lookup(
                    &request,
                    |spec, fp| {
                        if source_discovery_missed && fp == file_path && spec == specifier {
                            return None;
                        }
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
                apply_json_type_import_attribute_override(
                    &mut outcome,
                    module_specifier_has_type_json_import_attribute(&file.arena, *specifier_node),
                    file_path,
                    specifier,
                    options,
                    base_dir,
                    resolution_cache,
                    program_paths,
                );

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
                        if let Some(target_idx) = program_file_index.get_with_symlink_fallback(
                            &canonical,
                            resolved_path,
                            options,
                        ) {
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

                // Record the untyped-JS resolution target for the checker's
                // augmentation-site TS2665 check. Independent of `outcome.error`:
                // the same resolution produces TS7016 only under `noImplicitAny`.
                if let Some(ref untyped_path) = outcome.untyped_module_path {
                    untyped_module_paths.insert(
                        (file_idx, specifier.clone()),
                        untyped_path.to_string_lossy().into_owned(),
                    );
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

    let resolved_module_errors = Arc::new(resolved_module_errors);
    let resolved_module_request_errors = Arc::new(resolved_module_request_errors);
    let untyped_module_paths = Arc::new(untyped_module_paths);

    // Pre-compute per-file TS7016 diagnostics for CJS require() calls.
    // The driver's resolution pass detects untyped JS modules (TS7016) but the
    // checker's module-not-found path skips them because the module DID resolve.
    // For CJS require() calls (not import declarations), we emit TS7016 directly.
    //
    // Pure read-only per-file work (arena + pre-computed maps), so Rayon can
    // spread the scan across cores. On large repos this turns an N-file
    // sequential post-pass into an N-way parallel pass.
    let per_file_ts7016_diagnostics: Arc<Vec<Vec<Diagnostic>>> = Arc::new({
        use rayon::prelude::*;
        let _span = tracing::info_span!("per_file_ts7016_diagnostics", files = program.files.len())
            .entered();
        let has_cjs_require_specifier = cached_module_specifiers.iter().any(|specifiers| {
            specifiers.iter().any(|(_, _, import_kind, _)| {
                matches!(import_kind, tsz::module_resolver::ImportKind::CjsRequire)
            })
        });
        if !has_cjs_require_specifier {
            vec![Vec::new(); program.files.len()]
        } else {
            tsz::parallel::ensure_rayon_global_pool();
            program
                .files
                .par_iter()
                .enumerate()
                .map(|(file_idx, file)| {
                    let mut diags = Vec::new();
                    for (specifier, spec_node, import_kind, _) in
                        &cached_module_specifiers[file_idx]
                    {
                        if !matches!(import_kind, tsz::module_resolver::ImportKind::CjsRequire) {
                            continue;
                        }
                        if let Some(error) =
                            resolved_module_errors.get(&(file_idx, specifier.clone()))
                        {
                            if error.code != 7016 {
                                continue;
                            }
                            // Find the string literal argument of the require() call for the span.
                            let (start, length) = if let Some(node) = file.arena.get(*spec_node)
                                && let Some(call) = file.arena.get_call_expr(node)
                                && let Some(args) = call.arguments.as_ref()
                                && let Some(&arg_idx) = args.nodes.first()
                                && let Some(arg_node) = file.arena.get(arg_idx)
                            {
                                (arg_node.pos, arg_node.end.saturating_sub(arg_node.pos))
                            } else if let Some(node) = file.arena.get(*spec_node) {
                                (node.pos, node.end.saturating_sub(node.pos))
                            } else {
                                continue;
                            };
                            diags.push(Diagnostic::error(
                                &file.file_name,
                                start,
                                length,
                                &error.message,
                                error.code,
                            ));
                        }
                    }
                    diags
                })
                .collect()
        }
    });

    // Pre-compute per-file ESM/CJS module kind for resolution modes that honor
    // package.json "type" semantics. The checker uses this shared map for
    // ESM-vs-CJS-sensitive diagnostics such as TS1479 and TS1192 suppression.
    let file_is_esm_map: Arc<FxHashMap<String, bool>> = Arc::new({
        let resolution_kind = options.effective_module_resolution();
        let uses_package_type_module_kind = matches!(
            resolution_kind,
            crate::config::ModuleResolutionKind::Bundler
                | crate::config::ModuleResolutionKind::Node16
                | crate::config::ModuleResolutionKind::NodeNext
        );
        if uses_package_type_module_kind {
            let program_package_types: FxHashMap<PathBuf, bool> = program
                .files
                .iter()
                .filter_map(|file| {
                    let file_path = Path::new(&file.file_name);
                    if file_path.file_name().and_then(|name| name.to_str()) != Some("package.json")
                    {
                        return None;
                    }
                    let package_dir = file_path.parent()?.to_path_buf();
                    let text = file
                        .arena
                        .source_files
                        .first()
                        .map(|source_file| source_file.text.as_ref())?;
                    let package_type = serde_json::from_str::<serde_json::Value>(text)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("type")
                                .and_then(serde_json::Value::as_str)
                                .map(|value| value == "module")
                        })
                        .unwrap_or(false);
                    Some((package_dir, package_type))
                })
                .collect();
            let mut package_type_cache = ModuleResolutionCache::default();
            program
                .files
                .iter()
                .map(|file| {
                    let file_path = Path::new(&file.file_name);
                    let file_is_esm = match file_path.extension().and_then(|ext| ext.to_str()) {
                        Some("mts" | "mjs") => true,
                        Some("cts" | "cjs") => false,
                        _ => {
                            let mut current = file_path.parent();
                            let mut from_program_package_json = None;
                            while let Some(dir) = current {
                                if let Some(&is_esm) = program_package_types.get(dir) {
                                    from_program_package_json = Some(is_esm);
                                    break;
                                }
                                current = dir.parent();
                            }
                            from_program_package_json.unwrap_or_else(|| {
                                implied_resolution_mode_for_file_with_cache(
                                    file_path,
                                    base_dir,
                                    &mut package_type_cache,
                                ) == "import"
                            })
                        }
                    };
                    (file.file_name.replace('\\', "/"), file_is_esm)
                })
                .collect()
        } else {
            FxHashMap::default()
        }
    });

    SourceResolutionSetup {
        cached_module_specifiers,
        resolved_module_paths: Arc::new(resolved_module_paths),
        resolved_module_request_paths: Arc::new(resolved_module_request_paths),
        resolved_module_ts_extension_flags: Arc::new(resolved_module_ts_extension_flags),
        resolved_module_errors,
        resolved_module_request_errors,
        untyped_module_paths,
        resolved_modules_per_file,
        per_file_ts7016_diagnostics,
        file_is_esm_map,
    }
}
