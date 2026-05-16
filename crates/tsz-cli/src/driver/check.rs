//! Diagnostics collection and per-file checking orchestration for the compilation driver.

use super::check_module_graph::*;
use super::check_utils::*;
use super::*;
use tsz::checker::context::RequestCacheCounters;

const fn checker_resolution_mode_override(
    mode: Option<tsz::module_resolver::ImportingModuleKind>,
) -> Option<tsz::checker::context::ResolutionModeOverride> {
    match mode {
        Some(tsz::module_resolver::ImportingModuleKind::Esm) => {
            Some(tsz::checker::context::ResolutionModeOverride::Import)
        }
        Some(tsz::module_resolver::ImportingModuleKind::CommonJs) => {
            Some(tsz::checker::context::ResolutionModeOverride::Require)
        }
        None => None,
    }
}

const fn checker_resolution_request_kind(
    kind: tsz::module_resolver::ImportKind,
) -> tsz::checker::context::ResolutionRequestKind {
    match kind {
        tsz::module_resolver::ImportKind::EsmImport => {
            tsz::checker::context::ResolutionRequestKind::EsmImport
        }
        tsz::module_resolver::ImportKind::DynamicImport => {
            tsz::checker::context::ResolutionRequestKind::DynamicImport
        }
        tsz::module_resolver::ImportKind::CjsRequire => {
            tsz::checker::context::ResolutionRequestKind::CjsRequire
        }
        tsz::module_resolver::ImportKind::EsmReExport => {
            tsz::checker::context::ResolutionRequestKind::EsmReExport
        }
    }
}

fn checker_lookup_resolution_mode(
    module_resolver: &mut ModuleResolver,
    options: &ResolvedCompilerOptions,
    file_path: &Path,
    import_kind: tsz::module_resolver::ImportKind,
    resolution_mode_override: Option<tsz::module_resolver::ImportingModuleKind>,
) -> Option<tsz::checker::context::ResolutionModeOverride> {
    use tsz::module_resolver::{ImportKind, ImportingModuleKind, ModuleExtension};

    let mode = resolution_mode_override.unwrap_or_else(|| {
        match import_kind {
            // Mirror ModuleResolver::resolve_with_kind_and_module_kind() so request-keyed
            // checker maps line up with the actual lookup mode used by the resolver.
            ImportKind::DynamicImport => ImportingModuleKind::Esm,
            ImportKind::CjsRequire => ImportingModuleKind::CommonJs,
            ImportKind::EsmImport | ImportKind::EsmReExport => match options.checker.module {
                ModuleKind::Preserve => {
                    let extension = ModuleExtension::from_path(file_path);
                    if extension.forces_esm() {
                        ImportingModuleKind::Esm
                    } else if extension.forces_cjs() {
                        ImportingModuleKind::CommonJs
                    } else {
                        ImportingModuleKind::Esm
                    }
                }
                _ => module_resolver.get_importing_module_kind(file_path),
            },
        }
    });

    checker_resolution_mode_override(Some(mode))
}

pub(super) struct CollectDiagnosticsResult {
    pub diagnostics: Vec<Diagnostic>,
    pub request_cache_counters: RequestCacheCounters,
    /// Aggregate query-cache statistics from the selected checking path.
    pub query_cache_stats: Option<tsz_solver::QueryCacheStatistics>,
    /// Aggregate definition-store statistics (populated for `--extendedDiagnostics`).
    pub def_store_stats: Option<tsz_solver::StoreStatistics>,
    /// Module dependency graph statistics (populated for `--extendedDiagnostics`).
    pub module_dep_stats: Option<super::ModuleDependencyStats>,
}

#[derive(Default)]
pub(super) struct CheckerLibSet {
    pub(super) files: Vec<Arc<LibFile>>,
    pub(super) contexts: Arc<Vec<LibContext>>,
}

/// Check if a filename is a TypeScript declaration file (`.d.ts`, `.d.cts`,
/// `.d.mts`, or `.d.<ext>.ts`).
fn is_declaration_file(name: &str) -> bool {
    tsz::module_resolver::ModuleExtension::from_path(std::path::Path::new(name)).is_declaration()
}

#[cfg(test)]
thread_local! {
    static FILE_SESSION_REUSE_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn file_session_reuse_test_override() -> Option<bool> {
    FILE_SESSION_REUSE_TEST_OVERRIDE.with(std::cell::Cell::get)
}

fn file_session_reuse_requested() -> bool {
    #[cfg(test)]
    if let Some(enabled) = file_session_reuse_test_override() {
        return enabled;
    }

    std::env::var_os("TSZ_DISABLE_FILE_SESSION_REUSE").is_none()
}

fn parallel_file_session_reuse_requested() -> bool {
    #[cfg(test)]
    if let Some(enabled) = file_session_reuse_test_override() {
        return enabled;
    }

    if std::env::var_os("TSZ_DISABLE_FILE_SESSION_REUSE").is_some() {
        return false;
    }

    // `TSZ_FILE_SESSION_REUSE` used to opt into this path explicitly.
    // Keep treating it as an accepted compatibility knob while defaulting
    // to reuse when the global disable knob is not set.
    let _legacy_opt_in = std::env::var_os("TSZ_FILE_SESSION_REUSE").is_some();
    true
}

const FILE_SESSION_REUSE_PARALLEL_CHUNK_SIZE: usize = 8;

fn should_apply_duplicate_package_redirect(importing_file: &Path) -> bool {
    importing_file
        .components()
        .any(|component| component.as_os_str() == "node_modules")
}

/// Clone lib.d.ts files and create fresh checker-facing `LibContext` objects.
///
/// The binding pipeline mutates per-file binder state while injecting lib symbols into the
/// unified program. Reusing those same `LibFile` binders as checker lib contexts leaks that
/// binding-phase state into lib type resolution and can corrupt structural relations between
/// recursive lib types like `RegExpMatchArray`, `Promise<T>`, and `PromiseLike<T>`.
///
/// Build a fresh checker-facing lib set from the already-loaded lib sources so program
/// binding and checker lib resolution stay isolated without requiring disk reloads.
pub(super) fn load_checker_libs(lib_files: &[Arc<LibFile>]) -> CheckerLibSet {
    let files = parallel::clone_lib_files_for_checker(lib_files, false);
    let contexts = files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    CheckerLibSet {
        files,
        contexts: Arc::new(contexts),
    }
}

fn should_skip_type_checking_for_file(
    file_name: &str,
    options: &ResolvedCompilerOptions,
    is_default_lib: bool,
) -> bool {
    (options.skip_lib_check && is_declaration_file(file_name))
        || (options.skip_default_lib_check && is_default_lib)
}

fn program_has_real_syntax_errors(program: &MergedProgram) -> bool {
    program
        .files
        .iter()
        .flat_map(|file| file.parse_diagnostics.iter())
        .any(|diag| is_real_syntax_error(diag.code))
}

fn program_has_unsupported_js_root(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
) -> bool {
    !options.allow_js
        && program
            .files
            .iter()
            .any(|file| is_js_file(Path::new(&file.file_name)))
}

const fn is_reserved_type_name_declaration_diagnostic(code: u32) -> bool {
    matches!(code, 2427 | 2457)
}

/// Returns true if a TS2427 diagnostic message refers to a hard reserved
/// keyword that triggers a parser error in tsc (`void` or `null`). When such
/// an interface declaration is present in a source file, tsc only surfaces
/// the TS2427 for that hard-keyword interface and suppresses TS2427 for any
/// other reserved-name interfaces in the same file. This mirrors tsc's
/// behavior in `interfacesWithPredefinedTypesAsNames.ts` and similar tests.
fn is_hard_keyword_interface_name_2427(diag: &Diagnostic) -> bool {
    if diag.code != 2427 {
        return false;
    }
    diag.message_text == "Interface name cannot be 'void'."
        || diag.message_text == "Interface name cannot be 'null'."
}

fn keep_checker_diagnostic_when_program_has_real_syntax_errors(code: u32) -> bool {
    // tsc suppresses type-level semantic diagnostics when any source file in the
    // program has a real syntax error, but it still reports declaration-name
    // diagnostics such as TS2427/TS2457 alongside parse errors because the parser
    // accepts those names and defers validation to the checker.
    if code == 1315 {
        return false;
    }
    code < 2000
        || tsz::checker::diagnostics::is_js_grammar_diagnostic(code)
        || is_reserved_type_name_declaration_diagnostic(code)
}

/// `TS1xxx` codes that tsc routes through `getSemanticDiagnostics`. They are in
/// the parser-grammar range numerically but are emitted from the checker, so
/// unchecked JS files (no `checkJs`, or `// @ts-nocheck`) must not see them
/// even though `code < 2000` would otherwise let them through. Issue #3693.
const fn is_semantic_ts1xxx_suppressed_in_unchecked_js(code: u32) -> bool {
    matches!(
        code,
        1192 // Module '{0}' has no default export.
        | 1259 // Module '{0}' can only be default-imported using the '{1}' flag
    )
}

fn post_process_checker_diagnostics(
    checker_diagnostics: &mut Vec<Diagnostic>,
    file: &BoundFile,
    options: &ResolvedCompilerOptions,
    program_has_real_syntax_errors: bool,
    program_has_unsupported_js_root: bool,
    has_deprecation_diagnostics: bool,
) {
    let is_js = is_js_file(Path::new(&file.file_name));
    let has_ts_check_pragma = js_file_has_ts_check_pragma(file);
    let has_ts_nocheck_pragma = js_file_has_ts_nocheck_pragma(file);
    let should_filter_type_errors =
        is_js && (has_ts_nocheck_pragma || (!options.check_js && !has_ts_check_pragma));

    if should_filter_type_errors {
        // Keep syntax/semantic diagnostics (< 2000) and JS grammar diagnostics
        // (TS8xxx). When `checkJs` is NOT explicitly false (the default
        // no-checkJs mode), also allow the `plainJSErrors` codes that tsc
        // surfaces even in unchecked JS files. When `checkJs: false` is
        // explicitly set, suppress ALL semantic errors.
        //
        // Issue #3693: a few TS1xxx codes are semantic checker diagnostics
        // that tsc routes through `getSemanticDiagnostics`. Their numeric
        // code is < 2000 but they must NOT survive unchecked-JS filtering,
        // because tsc doesn't surface them in that mode either.
        checker_diagnostics.retain(|diag| {
            if is_semantic_ts1xxx_suppressed_in_unchecked_js(diag.code) {
                return false;
            }
            diag.code < 2000
                || tsz::checker::diagnostics::is_js_grammar_diagnostic(diag.code)
                || (!options.explicit_check_js_false && is_plain_js_allowed_code(diag.code))
        });
    }

    // For JS files, suppress checker-emitted TS1xxx grammar codes that tsc
    // does NOT emit for JavaScript files. tsc's grammar checks (emitted via
    // grammarErrorOnNode) are suppressed for TypeScript-only constructs in JS
    // files because its parser handles them leniently. Our parser doesn't
    // distinguish JS vs TS, so checker-side grammar errors leak through.
    // Only keep TS1xxx codes that tsc is known to emit for JS files.
    if is_js {
        checker_diagnostics.retain(|diag| {
            // Some semantic checker diagnostics live in the TS1xxx range. Keep
            // them for checked JS files even though the coarse parser-grammar
            // classifier also covers TS1xxx.
            if !should_filter_type_errors
                && (matches!(diag.code, 1361 | 1362)
                    || is_semantic_ts1xxx_suppressed_in_unchecked_js(diag.code))
            {
                return true;
            }
            if tsz::checker::diagnostics::is_parser_grammar_diagnostic(diag.code) {
                return is_ts1xxx_allowed_in_js(diag.code);
            }
            // Also suppress checker-emitted grammar codes outside the 1xxx range
            // that tsc doesn't emit for JS files.
            if is_checker_grammar_code_suppressed_in_js(diag.code) {
                return false;
            }
            true
        });
    }

    if program_has_real_syntax_errors {
        checker_diagnostics
            .retain(|diag| keep_checker_diagnostic_when_program_has_real_syntax_errors(diag.code));
    }

    if program_has_unsupported_js_root && !program_has_real_syntax_errors {
        // tsc reports program-level TS6504 for explicit JS/CJS roots when
        // allowJs is disabled, then skips downstream semantic checks.
        checker_diagnostics
            .retain(|diag| keep_checker_diagnostic_when_program_has_real_syntax_errors(diag.code));
    }

    // TS2754 ("super may not use type arguments") indicates a fundamental class
    // hierarchy error. tsc suppresses all other semantic diagnostics when TS2754
    // is present. TS2754 is emitted by the parser, so check parse diagnostics.
    let has_ts2754 = file.parse_diagnostics.iter().any(|d| d.code == 2754);
    if has_ts2754 {
        checker_diagnostics.retain(|diag| diag.code < 2000);
    }

    // When the file contains an `interface void {}` or `interface null {}`
    // declaration, tsc only emits TS2427 for that hard-keyword interface and
    // suppresses TS2427 for ANY other interfaces in the same file (including
    // ones with predefined-type names like `any`, `number`, etc.). This is
    // because tsc's parser produces a parse error for hard-keyword names,
    // which prevents the lazy diagnostic queue from running for the other
    // interface declarations. We don't currently emit a parse error in our
    // parser for `void`/`null` as interface names, so we model the same
    // suppression by filtering out non-hard-keyword TS2427 when a
    // hard-keyword TS2427 is present.
    let has_hard_keyword_ts2427 = checker_diagnostics
        .iter()
        .any(is_hard_keyword_interface_name_2427);
    if has_hard_keyword_ts2427 {
        checker_diagnostics.retain(|diag| {
            // Keep all non-TS2427 diagnostics untouched.
            if diag.code != 2427 {
                return true;
            }
            // Among TS2427, keep only the hard-keyword (`void`/`null`) ones.
            is_hard_keyword_interface_name_2427(diag)
        });
    }

    // When TS5107/TS5101 deprecation diagnostics are present, suppress the most
    // common type relationship errors that tsc would not emit. Parser errors
    // (<2000) are handled separately and not affected by this filter.
    if has_deprecation_diagnostics {
        // Type relationship errors to suppress when deprecation warnings are present
        const SUPPRESSED_TYPE_CODES: &[u32] = &[
            2322, // TS2322: Type not assignable
            2345, // TS2345: Argument not assignable
            2339, // TS2339: Property does not exist
            2343, // TS2343: Access modifier error
            2882, // TS2882: Cannot find module/type declarations for side-effect import
            2304, // TS2304: Cannot find name
            2307, // TS2307: Cannot find module
            7006, // TS7006: Parameter implicitly has 'any' type
            7005, // TS7005: Variable implicitly has 'any' type
            2323, // TS2323: Cannot redeclare exported variable
            2741, // TS2741: Missing properties
            2510, // TS2510: Cannot assign to read-only property
            2694, // TS2694: Namespace not found
            2531, // TS2531: Possibly null
            2532, // TS2532: Possibly undefined
            2533, // TS2533: Object is possibly null or undefined
            2564, // TS2564: Property has no initializer
            2454, // TS2454: Variable used before being assigned
            2403, // TS2403: Subsequent variable declarations must have same type
            2411, // TS2411: Property conflict
            2300, // TS2300: Duplicate identifier
        ];
        checker_diagnostics.retain(|diag| !SUPPRESSED_TYPE_CODES.contains(&diag.code));
    }

    // Suppress semantic errors that cascade from structural parse failures.
    // tsc sets per-node ThisNodeHasError flags and skips semantic checks on
    // error-recovery subtrees. We approximate this by suppressing semantic
    // diagnostics that are near a structural parse error (within a distance
    // window). Only structural parse failures (missing tokens, unexpected
    // tokens) trigger suppression — grammar checks like trailing commas or
    // strict mode violations don't cause AST malformation and shouldn't
    // suppress semantic errors.
    let structural_error_positions: Vec<u32> = file
        .parse_diagnostics
        .iter()
        .filter(|d| is_structural_parse_error(d.code))
        .map(|d| d.start)
        .collect();
    if !structural_error_positions.is_empty() {
        const MAX_CASCADE_DISTANCE: u32 = 300;
        checker_diagnostics.retain(|diag| {
            // Keep parse/grammar errors (1xxx) and JS grammar errors (8xxx)
            if diag.code < 2000 || tsz::checker::diagnostics::is_js_grammar_diagnostic(diag.code) {
                return true;
            }
            // Some semantic errors are deliberately emitted alongside
            // structural parse errors and must not be suppressed.
            // TS2427 / TS2457 are checker-side validation for reserved type names
            // in interface/type-alias declarations. TSC keeps them even when the
            // surrounding file also has structural parse errors.
            if is_reserved_type_name_declaration_diagnostic(diag.code) {
                return true;
            }
            // Suppress if a structural parse error is within the cascade window
            !structural_error_positions.iter().any(|&err_pos| {
                let dist = diag.start.abs_diff(err_pos);
                dist <= MAX_CASCADE_DISTANCE
            })
        });
    }
}

const LARGE_WILDCARD_BARREL_EXPORTS: usize = 32;

fn has_large_wildcard_barrel(program: &MergedProgram, work_items: &[usize]) -> bool {
    work_items.iter().any(|&file_idx| {
        program
            .files
            .get(file_idx)
            .and_then(|file| program.wildcard_reexports.get(&file.file_name))
            .is_some_and(|sources| sources.len() >= LARGE_WILDCARD_BARREL_EXPORTS)
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_diagnostics(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    cache: Option<&mut CompilationCache>,
    checker_libs: &CheckerLibSet,
    typescript_dom_replacement_globals: (bool, bool, bool),
    type_cache_output: &std::sync::Mutex<FxHashMap<PathBuf, TypeCache>>,
    has_deprecation_diagnostics: bool,
    collect_compile_stats: bool,
) -> CollectDiagnosticsResult {
    let _collect_span =
        tracing::info_span!("collect_diagnostics", files = program.files.len()).entered();
    // Production CLI semantic diagnostics are scheduled here. Lower-level
    // helpers in `tsz::parallel` stay reusable infrastructure unless this
    // driver opts into changed CLI behavior.
    #[cfg(not(target_arch = "wasm32"))]
    tsz::parallel::ensure_rayon_global_pool();

    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut request_cache_counters = RequestCacheCounters::default();
    let file_count = program.files.len();
    let mut used_paths = FxHashSet::with_capacity_and_hasher(file_count, Default::default());
    let mut cache = cache;
    let mut resolution_cache = ModuleResolutionCache::default();
    // Pre-size: each map ends up with exactly one entry per file. Without
    // capacity hints the inserts go through the standard power-of-two grow
    // path (~12 rehashes for a 6000-entry map). Driver setup runs once per
    // build, but on a 6000-file project the rehash cost is real startup
    // overhead.
    let mut program_paths = FxHashSet::with_capacity_and_hasher(file_count, Default::default());
    let mut canonical_to_file_name: FxHashMap<PathBuf, String> =
        FxHashMap::with_capacity_and_hasher(file_count, Default::default());
    let mut canonical_to_file_idx: FxHashMap<PathBuf, usize> =
        FxHashMap::with_capacity_and_hasher(file_count, Default::default());
    let program_has_real_syntax_errors = program_has_real_syntax_errors(program);
    let program_has_unsupported_js_root = program_has_unsupported_js_root(program, options);

    // TS6504: when allowJs is disabled, emit one error per explicit JS root file.
    // tsc includes the JS file in the program but rejects it with this diagnostic
    // and skips semantic checks for that file (the suppression is in
    // post_process_checker_diagnostics).
    if program_has_unsupported_js_root {
        for file in &program.files {
            if is_js_file(Path::new(&file.file_name)) {
                let mut ts6504 = Diagnostic::from_code(
                    diagnostic_codes::FILE_IS_A_JAVASCRIPT_FILE_DID_YOU_MEAN_TO_ENABLE_THE_ALLOWJS_OPTION,
                    "",
                    0,
                    0,
                    &[&file.file_name],
                );
                ts6504
                    .related_information
                    .push(DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Message,
                        code: diagnostic_codes::THE_FILE_IS_IN_THE_PROGRAM_BECAUSE,
                        file: String::new(),
                        start: 0,
                        length: 0,
                        message_text: "The file is in the program because:".to_string(),
                    });
                ts6504
                    .related_information
                    .push(DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Message,
                        code: diagnostic_codes::ROOT_FILE_SPECIFIED_FOR_COMPILATION,
                        file: String::new(),
                        start: 0,
                        length: 0,
                        message_text: "Root file specified for compilation".to_string(),
                    });
                diagnostics.push(ts6504);
            }
        }
    }

    {
        let _span = tracing::info_span!("build_program_path_maps", files = file_count).entered();
        for (idx, file) in program.files.iter().enumerate() {
            let canonical = normalize_resolved_path(Path::new(&file.file_name), options);
            program_paths.insert(canonical.clone());
            canonical_to_file_name.insert(canonical.clone(), file.file_name.clone());
            canonical_to_file_idx.insert(canonical, idx);
        }
    }

    // Create ModuleResolver instance for proper error reporting (TS2834, TS2835, TS2792, etc.)
    let mut module_resolver = ModuleResolver::new(options);

    // Cache module specifiers per file — collected once, reused in prepare_binders
    // and check_file_for_parallel to avoid 3× redundant AST traversals.
    type CachedModuleSpecifier = (
        String,
        tsz::parser::NodeIndex,
        tsz::module_resolver::ImportKind,
        Option<tsz::module_resolver::ImportingModuleKind>,
    );

    // AST traversal is pure read-only and embarrassingly parallel: each file
    // independently scans its own arena. Doing this up-front in parallel lets
    // the subsequent (sequential) module-resolution loop iterate over a
    // pre-built `Vec<Vec<...>>` instead of interleaving the AST scan with
    // the resolution-cache mutation. On large repos this turns N sequential
    // AST passes into one N-way parallel pass.
    let cached_module_specifiers: Vec<Vec<CachedModuleSpecifier>> = {
        use rayon::prelude::*;
        let _span =
            tracing::info_span!("collect_module_specifiers", files = program.files.len()).entered();
        program
            .files
            .par_iter()
            .map(|file| collect_module_specifiers(&file.arena, file.source_file))
            .collect()
    };

    // Duplicate package redirect map
    let package_redirects: FxHashMap<PathBuf, PathBuf> = {
        let file_names: Vec<String> = program.files.iter().map(|f| f.file_name.clone()).collect();
        build_duplicate_package_redirects(&file_names, options)
    };
    let module_specifier_count: usize = cached_module_specifiers.iter().map(Vec::len).sum();

    // Build resolved_module_paths map: (source_file_idx, specifier) -> target_file_idx
    // Also build resolved_module_errors map for specific error codes
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> =
        FxHashMap::with_capacity_and_hasher(module_specifier_count, Default::default());
    // Per-resolution `resolvedUsingTsExtension` flag — populated when the
    // resolver consumed a `.ts` extension via a literal package.json
    // exports/imports key. Consumed by the checker's TS2877 gate. This and the
    // error maps stay sparse: most programs resolve without these entries.
    let mut resolved_module_ts_extension_flags: FxHashMap<(usize, String), bool> =
        FxHashMap::default();
    let mut resolved_module_request_paths: FxHashMap<
        (
            usize,
            String,
            Option<tsz::checker::context::ResolutionModeOverride>,
            tsz::checker::context::ResolutionRequestKind,
        ),
        usize,
    > = FxHashMap::with_capacity_and_hasher(module_specifier_count, Default::default());
    let mut resolved_module_specifiers: FxHashSet<(usize, String)> =
        FxHashSet::with_capacity_and_hasher(module_specifier_count, Default::default());
    let mut resolved_module_errors: FxHashMap<
        (usize, String),
        tsz::checker::context::ResolutionError,
    > = FxHashMap::default();
    let mut resolved_module_request_errors: FxHashMap<
        (
            usize,
            String,
            Option<tsz::checker::context::ResolutionModeOverride>,
            tsz::checker::context::ResolutionRequestKind,
        ),
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

                let result = module_resolver.lookup(
                    &request,
                    |spec, fp| {
                        resolve_module_specifier(
                            fp,
                            spec,
                            options,
                            base_dir,
                            &mut resolution_cache,
                            &program_paths,
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
                    Some(&program_paths),
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
                        &mut resolution_cache,
                    )
                    && let Some(resolved_path) = resolve_module_specifier(
                        file_path,
                        specifier,
                        options,
                        base_dir,
                        &mut resolution_cache,
                        &program_paths,
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

                // Map resolved path to file index
                // NOTE: Only mark as resolved if there's NO error. When there's a resolution
                // error (TS2307, etc.), the module should NOT be in resolved_module_specifiers
                // so that the checker will emit the appropriate error.
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

    let resolved_module_paths = Arc::new(resolved_module_paths);
    let resolved_module_request_paths = Arc::new(resolved_module_request_paths);
    let resolved_module_ts_extension_flags = Arc::new(resolved_module_ts_extension_flags);
    let resolved_module_specifiers = Arc::new(resolved_module_specifiers);
    let resolved_module_errors = Arc::new(resolved_module_errors);
    let resolved_module_request_errors = Arc::new(resolved_module_request_errors);

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

    // Pre-compute per-file TS7016 diagnostics for CJS require() calls.
    // The driver's resolution pass detects untyped JS modules (TS7016) but the
    // checker's module-not-found path skips them because the module DID resolve.
    // For CJS require() calls (not import declarations), we emit TS7016 directly.
    //
    // Pure read-only per-file work (arena + pre-computed maps), so Rayon can
    // spread the scan across cores. On large repos this turns an N-file
    // sequential post-pass into an N-way parallel pass.
    let per_file_ts7016_diagnostics: Vec<Vec<Diagnostic>> = {
        use rayon::prelude::*;
        let _span = tracing::info_span!("per_file_ts7016_diagnostics", files = program.files.len())
            .entered();
        program
            .files
            .par_iter()
            .enumerate()
            .map(|(file_idx, file)| {
                let mut diags = Vec::new();
                for (specifier, spec_node, import_kind, _) in &cached_module_specifiers[file_idx] {
                    if !matches!(import_kind, tsz::module_resolver::ImportKind::CjsRequire) {
                        continue;
                    }
                    if let Some(error) = resolved_module_errors.get(&(file_idx, specifier.clone()))
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
    };
    let per_file_ts7016_diagnostics = Arc::new(per_file_ts7016_diagnostics);

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
                    (file.file_name.clone(), file_is_esm)
                })
                .collect()
        } else {
            FxHashMap::default()
        }
    });

    // The `--noCheck` short-circuit returns parse-only diagnostics and skips
    // the regular checker pipeline. That's correct when no declaration files
    // are being emitted, but `--noCheck --declaration` still needs the
    // checker's inferred type information so the declaration emitter can
    // print return types for unannotated functions, contextual types for
    // `const x = { a: 1 }`, etc. (#3733). When `emit_declarations` is set
    // we fall through to the regular pipeline (which still suppresses type
    // errors via the `if !no_check` guard around `check_source_file`); the
    // type_caches it produces feed declaration emit.
    if options.no_check && !options.emit_declarations {
        use rayon::prelude::*;

        let mut diagnostics: Vec<Diagnostic> = program
            .files
            .par_iter()
            .map(|file| {
                let mut file_diags = collect_no_check_file_diagnostics(
                    file,
                    options,
                    program_has_real_syntax_errors,
                );
                // tsc still reports the `--isolatedDeclarations` grammar
                // diagnostics (TS9007/TS9011/TS9012/etc.) under `--noCheck`
                // because they gate declaration emission, not type checking
                // (#3709). Run only the isolated-declaration grammar pass.
                if options.checker.isolated_declarations {
                    let mut binder = tsz_binder::state::BinderState::new();
                    binder.bind_source_file(&file.arena, file.source_file);
                    file_diags.extend(tsz::checker::run_isolated_declarations_pass(
                        &file.arena,
                        &binder,
                        file.source_file,
                        file.file_name.clone(),
                        options.checker.clone(),
                    ));
                }
                file_diags
            })
            .flatten()
            .collect();

        for (file_idx, file_diags) in per_file_ts7016_diagnostics.iter().enumerate() {
            diagnostics.extend(file_diags.iter().cloned());
            if let Some(file) = program.files.get(file_idx) {
                used_paths.insert(PathBuf::from(&file.file_name));
            }
        }

        if let Some(c) = cache {
            c.type_caches.retain(|path, _| used_paths.contains(path));
            c.diagnostics.retain(|path, _| used_paths.contains(path));
            c.export_hashes.retain(|path, _| used_paths.contains(path));
        }

        diagnostics.extend(detect_missing_tslib_helper_diagnostics(
            program,
            options,
            base_dir,
            &file_is_esm_map,
        ));

        let module_dep_stats = if collect_compile_stats {
            Some(compute_module_dependency_stats(
                program.files.len(),
                resolved_module_paths.as_ref(),
            ))
        } else {
            None
        };

        return CollectDiagnosticsResult {
            diagnostics,
            request_cache_counters,
            query_cache_stats: Some(tsz_solver::QueryCacheStatistics::default()),
            def_store_stats: None,
            module_dep_stats,
        };
    }

    // `skipLibCheck` skips semantic checking for declaration files, but the
    // normal checker setup below still builds project-wide checker state before
    // discovering that every work item is skipped. Utility-type packages such
    // as type-fest are often pure `.d.ts` projects with `skipLibCheck`; for
    // pure no-emit checks, parse/module diagnostics are the only remaining
    // output. Return before constructing checker binders, `ProgramContext`, the
    // shared `DefinitionStore`, and lib recheck baselines. Mixed projects stay
    // on the normal path so `.ts` files can still consume declaration exports.
    if options.no_emit
        && options.skip_lib_check
        && !options.emit_declarations
        && program
            .files
            .iter()
            .all(|file| is_declaration_file(&file.file_name))
    {
        use rayon::prelude::*;

        let mut diagnostics: Vec<Diagnostic> = program
            .files
            .par_iter()
            .map(|file| {
                collect_no_check_file_diagnostics(file, options, program_has_real_syntax_errors)
            })
            .flatten()
            .collect();

        for (file_idx, file_diags) in per_file_ts7016_diagnostics.iter().enumerate() {
            diagnostics.extend(file_diags.iter().cloned());
            if let Some(file) = program.files.get(file_idx) {
                used_paths.insert(PathBuf::from(&file.file_name));
            }
        }

        if let Some(c) = cache {
            c.type_caches.retain(|path, _| used_paths.contains(path));
            c.diagnostics.retain(|path, _| used_paths.contains(path));
            c.export_hashes.retain(|path, _| used_paths.contains(path));
        }

        diagnostics.extend(detect_missing_tslib_helper_diagnostics(
            program,
            options,
            base_dir,
            &file_is_esm_map,
        ));

        let module_dep_stats = if collect_compile_stats {
            Some(compute_module_dependency_stats(
                program.files.len(),
                resolved_module_paths.as_ref(),
            ))
        } else {
            None
        };

        return CollectDiagnosticsResult {
            diagnostics,
            request_cache_counters,
            query_cache_stats: Some(tsz_solver::QueryCacheStatistics::default()),
            def_store_stats: None,
            module_dep_stats,
        };
    }

    // Pre-compute merged augmentations once for all binder reconstruction paths.
    let merged_augmentations = MergedAugmentations::from_program(program);
    let can_recheck_checker_libs =
        !options.no_check && !options.skip_lib_check && !checker_libs.files.is_empty();
    let affected_lib_interfaces = if can_recheck_checker_libs {
        affected_lib_interface_names(program, checker_libs)
    } else {
        FxHashSet::default()
    };
    let affected_lib_extension_interfaces = if can_recheck_checker_libs {
        affected_lib_extension_interface_names(program, checker_libs, &affected_lib_interfaces)
    } else {
        FxHashSet::default()
    };
    let baseline_lib_datetimeformatpart_interfaces = if can_recheck_checker_libs {
        baseline_lib_datetimeformatpart_spelling_interface_names(checker_libs)
    } else {
        FxHashSet::default()
    };

    // Pre-create all binders for cross-file resolution.
    let all_binders: Arc<Vec<Arc<BinderState>>> = {
        use rayon::prelude::*;
        let _span =
            tracing::info_span!("build_cross_file_binders", files = program.files.len()).entered();
        Arc::new(
            program
                .files
                .par_iter()
                .enumerate()
                .map(|(file_idx, file)| {
                    Arc::new(create_cross_file_lookup_binder_with_augmentations(
                        file,
                        program,
                        file_idx,
                        &merged_augmentations,
                    ))
                })
                .collect(),
        )
    };

    // Extract is_external_module from BoundFile to preserve state across file bindings.
    // This fixes TS2664 which requires accurate per-file is_external_module values.
    let is_external_module_by_file: Arc<rustc_hash::FxHashMap<String, bool>> = Arc::new(
        program
            .files
            .iter()
            .map(|file| (file.file_name.clone(), file.is_external_module))
            .collect(),
    );

    // Collect all arenas for cross-file resolution.
    let all_arenas: Arc<Vec<Arc<NodeArena>>> = Arc::new({
        let _span = tracing::info_span!("collect_all_arenas").entered();
        program
            .files
            .iter()
            .map(|file| Arc::clone(&file.arena))
            .collect()
    });
    // Build symbol → file_idx mapping in O(symbols + files) instead of
    // O(symbols × files). Previously each symbol scanned `all_arenas` linearly
    // looking for the matching `Arc<NodeArena>` via `ptr_eq` — on a large
    // project (~100K-500K symbols × 6000+ files) that exploded into
    // billion-scale pointer comparisons just to populate the map.
    //
    // `Arc::ptr_eq` compares the inner allocation address, so the raw pointer
    // value uniquely identifies an arena. A small one-shot pointer→idx
    // hashmap drops the per-symbol cost from O(N_files) to O(1).
    let symbol_file_targets: Arc<Vec<(tsz::binder::SymbolId, usize)>> = {
        let _span = tracing::info_span!(
            "build_symbol_file_targets",
            symbols = program.symbol_arenas.len(),
            files = all_arenas.len()
        )
        .entered();
        let arena_ptr_to_idx: FxHashMap<*const tsz::parser::NodeArena, usize> = all_arenas
            .iter()
            .enumerate()
            .map(|(idx, arena)| (Arc::as_ptr(arena), idx))
            .collect();
        Arc::new(
            program
                .symbol_arenas
                .iter()
                .filter_map(|(sym_id, arena)| {
                    arena_ptr_to_idx
                        .get(&Arc::as_ptr(arena))
                        .map(|&file_idx| (*sym_id, file_idx))
                })
                .collect(),
        )
    };

    // Propagate noUncheckedIndexedAccess to the TypeInterner before creating the
    // QueryCache.  The `with_options` constructor intentionally skips this (to avoid
    // repeated writes from each per-file checker), so we set it once here.
    program
        .type_interner
        .set_no_unchecked_indexed_access(options.checker.no_unchecked_indexed_access);
    // Propagate exactOptionalPropertyTypes to the TypeInterner so that solver-side
    // queries (e.g. index-signature inference) see the same flag as the checker.
    program
        .type_interner
        .set_exact_optional_property_types(options.checker.exact_optional_property_types);

    // Create a shared QueryCache for memoized evaluate_type/is_subtype_of calls.
    let query_cache = QueryCache::new(&program.type_interner);

    // Pre-compute declared modules from skeleton when available.
    // This avoids re-scanning all binders for declared/ambient module names in
    // each checker's `set_all_binders` call — the skeleton captured this data
    // during the parallel parse/bind phase.
    //
    // When no skeleton exists (small projects, sequential mode, tests), fall
    // back to the merged program-wide `declared_modules` set. This is the
    // same merged data the (now-empty) per-binder `declared_modules` used to
    // hold; consumers route through `ctx.declared_modules_contains` which
    // prefers `global_declared_modules`. Without this fallback, ambient
    // module suppression in `import_declaration` regresses (TS2307 false
    // positives on declared modules) for non-skeleton paths.
    let skeleton_declared_modules: Option<Arc<tsz::checker::context::GlobalDeclaredModules>> =
        if let Some(skel) = program.skeleton_index.as_ref() {
            let (exact, patterns) = skel.build_declared_module_sets();
            Some(Arc::new(
                tsz::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns),
            ))
        } else if !program.declared_modules.is_empty()
            || !program.shorthand_ambient_modules.is_empty()
        {
            Some(Arc::new(
                tsz::checker::context::GlobalDeclaredModules::from_module_names(
                    program
                        .declared_modules
                        .iter()
                        .chain(program.shorthand_ambient_modules.iter()),
                ),
            ))
        } else {
            None
        };

    // Pre-compute expando index from skeleton when available.
    // This avoids re-scanning all binders for expando property assignments.
    let skeleton_expando_index: Option<Arc<FxHashMap<String, FxHashSet<String>>>> = program
        .skeleton_index
        .as_ref()
        .map(|skel| Arc::clone(&skel.expando_properties));

    // Phase 2 step 2: pre-compute the merged module-augmentations index from
    // skeleton data. The skeleton recorded each augmenting declaration's name
    // + StableLocation at extract time; this projection rehydrates them into
    // the legacy `Vec<(file_idx, ModuleAugmentation)>` shape so checker
    // consumers (`module_augmentation.rs`, `property_access_augmentation.rs`)
    // see no behavior change. The legacy per-binder loop in
    // `ProgramContext::build_global_indices` is skipped when this is `Some`.
    let skeleton_module_augmentations_index: Option<
        tsz::checker::context::GlobalModuleAugmentationsIndex,
    > = program
        .skeleton_index
        .as_ref()
        .map(|skel| Arc::new(skel.build_module_augmentations_index(&all_arenas)));

    // Phase 2 step 3: pre-compute the merged augmentation-targets index from
    // skeleton data. The skeleton recorded each `(symbol, module_spec)` pair
    // (with a StableLocation) at extract time; this projection rehydrates
    // them into the legacy `Vec<(SymbolId, file_idx)>` shape so checker
    // consumers (`module_augmentation.rs`) see no behavior change. The legacy
    // per-binder loop in `ProgramContext::build_global_indices` is skipped when
    // this is `Some`.
    let skeleton_augmentation_targets_index: Option<
        tsz::checker::context::GlobalAugmentationTargetsIndex,
    > = program
        .skeleton_index
        .as_ref()
        .map(|skel| Arc::new(skel.build_augmentation_targets_index()));

    // Phase 2 step 4: pre-compute the merged module-binder index from
    // skeleton data. The skeleton recorded each file's `module_exports` keys
    // at extract time; this projection rebuilds the legacy
    // `module_spec -> Vec<file_idx>` map (including the de-quoted normalized
    // variant) so checker consumers (`import/declaration.rs`,
    // `module_entity.rs`, `type_resolution/module.rs`) see no behavior
    // change. The legacy `module_binder_index` push lines inside the
    // per-binder `module_exports.iter()` loop in
    // `ProgramContext::build_global_indices` are skipped when this is `Some`.
    let skeleton_module_binder_index: Option<Arc<FxHashMap<String, Vec<usize>>>> = program
        .skeleton_index
        .as_ref()
        .map(|skel| Arc::new(skel.build_module_binder_index()));

    // Phase 2 step 6: pre-compute the merged module-exports index from
    // skeleton data + the post-merge `program.module_exports` map. The
    // skeleton recorded each file's `(spec, [export_name])` entries at
    // extract time; this projection rebuilds the legacy
    // `spec -> export_name -> Vec<(file_idx, SymbolId)>` map by resolving
    // SymbolIds against `program.module_exports` (which holds globally-
    // remapped post-merge IDs) so checker consumers (`type_only.rs`,
    // `state/type_resolution/module.rs`, `state/type_resolution/import_type.rs`)
    // see no behavior change. The legacy inner
    // `for (export_name, sym_id) in exports.iter()` push loop in
    // `ProgramContext::build_global_indices` is skipped when this is `Some`.
    //
    // SymbolId-coupling note: unlike PR #1145 (file-locals index, closed for
    // a regression), this projection does NOT extract pre-merge local
    // SymbolIds from the skeleton — it looks them up in the post-merge
    // `program.module_exports`. The skeleton only records name strings.
    let skeleton_module_exports_index: Option<tsz::checker::context::GlobalModuleExportsIndex> =
        program
            .skeleton_index
            .as_ref()
            .map(|skel| Arc::new(skel.build_module_exports_index(&program.module_exports)));

    // Build the project-wide shared environment once for all checkers (prime, parallel, sequential).
    // build_global_indices computes the 4 binder-derived indices once here so that
    // per-file checker creation via apply_to skips the O(N) binder scans.
    // Wrap the merged program-wide re-export maps in `Arc` once so N
    // cross-file lookup binders can share the single allocation instead of
    // each deep-cloning a copy. Cross-file consumers read these via
    // `ctx.reexports_for_file` / `wildcard_reexports_for_file`.
    // `program.reexports` is already `Arc`-wrapped on `MergedProgram`; cheap atomic clone.
    let program_reexports = Arc::clone(&program.reexports);
    // `program.wildcard_reexports` and `program.wildcard_reexports_type_only`
    // are already `Arc`-wrapped on `MergedProgram`; cheap atomic clone.
    let program_wildcard_reexports = Arc::clone(&program.wildcard_reexports);
    let program_wildcard_reexports_type_only = Arc::clone(&program.wildcard_reexports_type_only);
    // `program.module_exports` is already `Arc`-wrapped on `MergedProgram`;
    // cheap atomic clone for ProgramContext install.
    let program_module_exports = Arc::clone(&program.module_exports);
    // Same rationale for `program.cross_file_node_symbols`: the merged
    // program already owns the outer map behind `Arc`, so installing it into
    // the shared ProgramContext is an O(1) clone instead of deep-cloning the
    // `FxHashMap<usize, Arc<...>>` before re-sharing it.
    let program_cross_file_node_symbols = Arc::clone(&program.cross_file_node_symbols);
    // Same rationale for `program.alias_partners`: a single shared
    // FxHashMap<SymbolId, SymbolId> beats N per-binder deep-clones.
    let program_alias_partners = Arc::clone(&program.alias_partners);

    let mut program_context = tsz::checker::context::ProgramContext {
        lib_contexts: Arc::clone(&checker_libs.contexts),
        all_arenas: Arc::clone(&all_arenas),
        all_binders: Arc::clone(&all_binders),
        skeleton_declared_modules,
        skeleton_expando_index,
        skeleton_module_augmentations_index,
        skeleton_augmentation_targets_index,
        skeleton_module_binder_index,
        skeleton_module_exports_index,
        symbol_file_targets: Arc::clone(&symbol_file_targets),
        resolved_module_paths: Arc::clone(&resolved_module_paths),
        resolved_module_request_paths: Arc::clone(&resolved_module_request_paths),
        resolved_module_ts_extension_flags: Arc::clone(&resolved_module_ts_extension_flags),
        resolved_module_errors: Arc::clone(&resolved_module_errors),
        resolved_module_request_errors: Arc::clone(&resolved_module_request_errors),
        is_external_module_by_file: Arc::clone(&is_external_module_by_file),
        file_is_esm_map: Arc::clone(&file_is_esm_map),
        typescript_dom_replacement_globals,
        has_deprecation_diagnostics,
        program_reexports: Some(program_reexports),
        program_wildcard_reexports: Some(program_wildcard_reexports),
        program_wildcard_reexports_type_only: Some(program_wildcard_reexports_type_only),
        program_module_exports: Some(program_module_exports),
        program_cross_file_node_symbols: Some(program_cross_file_node_symbols),
        program_alias_partners: Some(program_alias_partners),
        cross_file_type_params_cache: std::env::var_os("TSZ_CROSS_FILE_TYPE_PARAMS_CACHE")
            .map(|_| Arc::new(dashmap::DashMap::new())),
        ..Default::default()
    };
    // Use fingerprint-aware rebuild when a skeleton index is available.
    // On the first build this always rebuilds; on subsequent incremental builds
    // with the same skeleton fingerprint the O(N) binder scan is skipped.
    if let Some(ref skel) = program.skeleton_index {
        program_context.build_global_indices_if_changed(skel.fingerprint);
    } else {
        program_context.build_global_indices();
    }
    // Build the shared SymbolId→file-index map once; shared via Arc across all checkers.
    program_context.build_global_symbol_file_index();

    // Create a shared DefinitionStore for all parallel checkers.
    // CRITICAL: All parallel checkers MUST share the same DefinitionStore so that
    // DefId allocation is globally unique. Without this, independent DefId sequences
    // in separate checkers cause TypeId collisions via Lazy(DefId) interning.
    {
        let shared_store = Arc::new(
            tsz_solver::def::DefinitionStore::from_semantic_defs_with_overlays(
                &program.semantic_defs,
                program.files.iter().map(|file| file.semantic_defs.as_ref()),
                |s| program.type_interner.intern_string(s),
            ),
        );
        shared_store.init_file_locks(program.files.len());
        program_context.shared_definition_store = Some(shared_store);
    }

    // Prime Array<T> base type with global augmentations before any file checks.
    // The prime checker uses the shared DefinitionStore (via program_context.apply_to).
    if !program.files.is_empty() && !checker_libs.contexts.is_empty() {
        let prime_idx = 0;
        let file = &program.files[prime_idx];
        let binder = parallel::create_binder_from_bound_file(file, program, prime_idx);
        let mut checker = CheckerState::with_options(
            &file.arena,
            &binder,
            &query_cache,
            file.file_name.clone(),
            &options.checker,
        );
        program_context.apply_to(&mut checker.ctx);
        checker.prime_boxed_types();
    }

    // PERF: the post-merge default-lib recheck (collect baseline + per-lib
    // check + subtract) only produces diagnostics when user code merges into
    // a global lib interface OR extends a lib interface that contributes
    // user-relevant members. When both `affected_lib_interfaces` and
    // `affected_lib_extension_interfaces` are empty, the per-lib check
    // diagnostic set equals the baseline set: their subtraction is empty
    // and no user-induced diagnostics can surface. Skip the baseline pass
    // entirely in that case — and skip the per-lib check loops below by
    // gating them on the same condition. For typical single-file or
    // module-only TS files (no `declare global`, no interface that augments
    // a lib type) this removes a fixed ~30-lib-file recheck tax that was
    // dominating the per-invocation floor (~380–430ms on tiny files).
    let needs_lib_recheck = can_recheck_checker_libs
        && (!affected_lib_interfaces.is_empty() || !affected_lib_extension_interfaces.is_empty());
    let baseline_lib_diagnostics = if needs_lib_recheck {
        collect_checker_lib_baseline_fingerprints(
            program,
            options,
            checker_libs,
            &affected_lib_interfaces,
            &affected_lib_extension_interfaces,
            &program_context,
        )
    } else {
        FxHashSet::default()
    };
    let baseline_lib_datetimeformatpart_diagnostics = if can_recheck_checker_libs
        && !options.lib_is_default
        && !has_esnext_umbrella_lib(checker_libs)
        && should_preserve_datetimeformatpart_spelling_baseline(checker_libs)
        && !baseline_lib_datetimeformatpart_interfaces.is_empty()
    {
        let mut diagnostics = collect_checker_lib_baseline_diagnostics_for_codes(
            program,
            options,
            checker_libs,
            &baseline_lib_datetimeformatpart_interfaces,
            &FxHashSet::default(),
            &program_context,
            &[2552],
        );
        diagnostics.retain(is_datetimeformatpart_spelling_baseline_diagnostic);
        diagnostics
    } else {
        Vec::new()
    };

    // --- SMART INVALIDATION: Work Queue Algorithm ---
    // Only type-check files that have changed or depend on files with changed export signatures

    let mut work_queue: VecDeque<usize> = VecDeque::new();
    let mut checked_files: FxHashSet<usize> = FxHashSet::default();

    // Mark all files as used for cache cleanup
    for (idx, file) in program.files.iter().enumerate() {
        let file_path = PathBuf::from(&file.file_name);
        used_paths.insert(file_path.clone());

        // Check if file has cached type information
        // If no cache or cache miss, file needs to be checked
        let needs_check = cache
            .as_deref()
            .is_none_or(|c| !c.type_caches.contains_key(&file_path)); // No cache at all -> check everything

        if needs_check {
            work_queue.push_back(idx);
            checked_files.insert(idx);
        }
    }

    // --- FILE CHECKING ---
    //
    // CLI semantic diagnostics are scheduled here rather than through the core
    // `check_files_parallel` helper. That keeps command-mode policy, diagnostic
    // filtering, cache invalidation, and checker reuse in the driver.
    //
    // Two driver paths:
    // 1. Non-cached (first build, CI): Check ALL files in parallel using rayon.
    //    No dependency cascade needed since we're checking everything.
    // 2. Cached (watch mode): Sequential work queue with export-hash-based
    //    dependency cascade for incremental invalidation.

    let checker_lib_file_env = CheckerLibFileCheckEnv {
        program,
        options,
        checker_libs,
        affected_interfaces: &affected_lib_interfaces,
        extension_interfaces: &affected_lib_extension_interfaces,
        merged_augmentations: &merged_augmentations,
        program_context: &program_context,
        program_has_real_syntax_errors,
        program_has_unsupported_js_root,
    };

    let (query_cache_stats, aggregated_ds_stats) = if cache.is_none() {
        // --- PARALLEL PATH: No cache, check all files concurrently ---
        let _parallel_span =
            tracing::info_span!("parallel_check_files", files = work_queue.len()).entered();

        let no_check = options.no_check;
        let check_js = options.check_js;
        let explicit_check_js_false = options.explicit_check_js_false;
        let skip_lib_check = options.skip_lib_check;
        let compiler_options = options.checker.clone();
        let mut work_items: Vec<usize> = Vec::with_capacity(work_queue.len());
        let mut skipped_file_diagnostics: Vec<Vec<Diagnostic>> = Vec::new();
        for file_idx in work_queue {
            let file = &program.files[file_idx];
            if should_skip_type_checking_for_file(&file.file_name, options, false) {
                let mut file_diags = collect_no_check_file_diagnostics(
                    file,
                    options,
                    program_has_real_syntax_errors,
                );
                file_diags.extend(per_file_ts7016_diagnostics[file_idx].iter().cloned());
                skipped_file_diagnostics.push(file_diags);
            } else {
                work_items.push(file_idx);
            }
        }
        diagnostics.extend(skipped_file_diagnostics.into_iter().flatten());

        let build_checker_binder = |file_idx: usize| {
            let file = &program.files[file_idx];
            let mut binder = create_binder_from_bound_file_with_augmentations(
                file,
                program,
                file_idx,
                &merged_augmentations,
            );

            // Bridge raw module specifiers to resolved export tables using the
            // pre-computed resolved_module_paths map (no FS calls needed).
            for (specifier, _, _, _) in &cached_module_specifiers[file_idx] {
                if let Some(&target_idx) = resolved_module_paths.get(&(file_idx, specifier.clone()))
                {
                    propagate_module_export_maps(
                        &mut binder,
                        specifier,
                        target_idx,
                        program,
                        &resolved_module_paths,
                    );
                }
            }
            binder
        };

        // `TypeCache` is consumed by the emit pipeline (JS or declaration
        // files). For a pure `--noEmit` run that does not also request
        // declarations the cache is never read, but extracting one per file
        // pins several hash maps per file in memory throughout the whole
        // check — on a 6000-file repo that grew to ~10 GB RSS and got the
        // process killed by macOS jetsam before any diagnostics emitted.
        // Skip extraction in that case and let per-file state drop as soon
        // as checking finishes.
        let extract_type_cache = !options.no_emit || options.emit_declarations;
        let shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>> =
            Arc::new(dashmap::DashMap::new());

        // Create shared cross-file query cache for multi-file projects.
        // Eliminates redundant type evaluations and relation checks across files.
        let shared_query_cache = if work_items.len() > 1 {
            Some(tsz_solver::SharedQueryCache::new())
        } else {
            None
        };

        let check_file_with_fresh_checker = |file_idx: usize| {
            let binder = build_checker_binder(file_idx);
            let context = CheckFileForParallelContext {
                file_idx,
                binder,
                program,
                compiler_options: &compiler_options,
                program_context: &program_context,
                resolved_modules_per_file: &resolved_modules_per_file,
                shared_lib_cache: Arc::clone(&shared_lib_cache),
                shared_query_cache: shared_query_cache.as_ref(),
                no_check,
                check_js,
                explicit_check_js_false,
                skip_lib_check,
                program_has_real_syntax_errors,
                program_has_unsupported_js_root,
                extract_type_cache,
            };
            check_file_for_parallel(context)
        };

        // Check all files in parallel — each file gets its own CheckerState and QueryCache.
        // TypeInterner (DashMap) is thread-safe; QueryCache uses RefCell/Cell per-thread.
        #[cfg(not(target_arch = "wasm32"))]
        let file_results: Vec<CheckFileResult> = {
            use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
            // Use sequential checking for small projects to avoid Rayon overhead
            // and non-deterministic false positives from concurrent type
            // interning. The `TypeInterner` uses `DashMap` for thread-safe
            // access, but concurrent type evaluation can still observe
            // dependency and lib/package declaration shapes in scheduler order.
            //
            // A slightly wider small-project lane also covers cross-file JSX
            // namespace/global declaration discovery and checked-JS
            // CommonJS/JSDoc constructor evidence. Importer files can otherwise
            // observe incomplete dependency shapes and emit flaky TS2339
            // diagnostics.
            let reuse_requested = file_session_reuse_requested();
            let parallel_reuse_requested = parallel_file_session_reuse_requested();
            let has_parallel_order_sensitive_global_lib =
                has_parallel_order_sensitive_global_lib(checker_libs);
            let use_sequential_checking = work_items.len() <= 32
                || has_large_wildcard_barrel(program, &work_items)
                // DOM-style global declarations are order-sensitive with
                // multiple concurrent checker contexts. Keep those projects
                // on the deterministic single-worker path until global
                // lookup state is fully parallel-stable.
                || has_parallel_order_sensitive_global_lib
                // Fresh per-file checkers can observe project-level lib/global
                // state in scheduler order when run concurrently. If the
                // session-reuse path is explicitly disabled, keep the fallback
                // deterministic by using fresh checkers sequentially.
                || !reuse_requested;
            // T2.1.B (`PERFORMANCE_PLAN.md` §6 PR table): by default,
            // the sequential no-emit path constructs one `CheckerState`
            // and re-targets it across files via
            // `CheckerContext::switch_to_file` instead of constructing
            // one per file. `TSZ_DISABLE_FILE_SESSION_REUSE=1` opts out.
            // This flag applies to the sequential branch here; the
            // parallel branch below has its own chunked worker-reuse path.
            // `extract_type_cache=true` (set when `--emit` or
            // `--declaration` is on) consumes the `CheckerState` per
            // file via `extract_cache(self)`. The reuse path holds
            // ONE `CheckerState` across the whole loop, so it can't
            // call the consuming variant. Pinning `--noEmit` runs is
            // exactly the bench/profiling scenario T2.1.B is built
            // for, so this restriction matches the use case rather
            // than narrowing it.
            let use_file_session_reuse =
                use_sequential_checking && !extract_type_cache && reuse_requested;
            if use_file_session_reuse {
                check_files_sequentially_with_reuse(
                    &work_items,
                    program,
                    &compiler_options,
                    &program_context,
                    &resolved_modules_per_file,
                    Arc::clone(&shared_lib_cache),
                    shared_query_cache.as_ref(),
                    no_check,
                    check_js,
                    explicit_check_js_false,
                    skip_lib_check,
                    program_has_real_syntax_errors,
                    program_has_unsupported_js_root,
                    extract_type_cache,
                    build_checker_binder,
                )
            } else if !use_sequential_checking && !extract_type_cache && parallel_reuse_requested {
                // T2.1.C follow-up: reuse is now also default-on in the
                // parallel no-emit lane, with `TSZ_DISABLE_FILE_SESSION_REUSE=1`
                // as the shared opt-out across sequential + parallel paths.
                tsz::parallel::ensure_rayon_global_pool();
                check_files_in_parallel_chunks_with_reuse(
                    &work_items,
                    program,
                    &compiler_options,
                    &program_context,
                    &resolved_modules_per_file,
                    Arc::clone(&shared_lib_cache),
                    shared_query_cache.as_ref(),
                    no_check,
                    check_js,
                    explicit_check_js_false,
                    skip_lib_check,
                    program_has_real_syntax_errors,
                    program_has_unsupported_js_root,
                    extract_type_cache,
                    FILE_SESSION_REUSE_PARALLEL_CHUNK_SIZE,
                    &build_checker_binder,
                )
            } else if use_sequential_checking {
                work_items
                    .iter()
                    .map(|&file_idx| check_file_with_fresh_checker(file_idx))
                    .collect()
            } else {
                tsz::parallel::ensure_rayon_global_pool();
                // PERF: force `with_min_len(1)` so rayon's work-stealing
                // scheduler doesn't pre-chunk the file list into large blocks.
                // Per-file check time varies wildly (a file with one type
                // alias is ~ms; a file that triggers a deep
                // `delegate_cross_arena_symbol_resolution` cascade through
                // ts-essentials/react.d.ts can take seconds). With default
                // chunking the worker that draws the heavy chunk gates the
                // entire batch. Sample profiles on subset3 (1429 files, only
                // one worker active for the bulk of the check phase)
                // confirmed this skew. Fine-grained stealing lets idle
                // workers grab one file at a time from the busy worker's
                // queue.
                work_items
                    .par_iter()
                    .with_min_len(1)
                    .map(|&file_idx| check_file_with_fresh_checker(file_idx))
                    .collect()
            }
        };

        #[cfg(target_arch = "wasm32")]
        let file_results: Vec<CheckFileResult> = work_items
            .iter()
            .map(|&file_idx| check_file_with_fresh_checker(file_idx))
            .collect();

        // Aggregate per-file query cache statistics. DefinitionStore stats
        // come from the shared store computed once after the loop (workers
        // all see the same shared store, so summing per-file was both
        // wasted work and N× inflated).
        let mut parallel_qc_stats = tsz_solver::QueryCacheStatistics::default();
        let parallel_ds_stats = tsz_solver::StoreStatistics::default();
        {
            let mut tc_out = type_cache_output
                .lock()
                .expect("type_cache_output mutex poisoned");
            for (idx, (file_diags, type_cache, file_counters, qc_stats, _ds_stats)) in
                file_results.into_iter().enumerate()
            {
                diagnostics.extend(file_diags);
                // Inject pre-computed TS7016 diagnostics for CJS require() calls.
                let file_idx = work_items[idx];
                diagnostics.extend(per_file_ts7016_diagnostics[file_idx].iter().cloned());
                request_cache_counters.merge(file_counters);
                parallel_qc_stats.merge(&qc_stats);
                if let Some(tc) = type_cache {
                    let file_path = PathBuf::from(&program.files[file_idx].file_name);
                    tc_out.insert(file_path, tc);
                }
            }
        }
        // PERF: see `needs_lib_recheck` above — when no user-defined global
        // interface merges into a lib interface and no user interface extends a
        // lib interface that contributes user-relevant members, the per-lib
        // check produces only baseline diagnostics that get subtracted away.
        // Skip the loop entirely.
        if needs_lib_recheck {
            for lib_idx in 0..checker_libs.files.len() {
                let query_cache = if let Some(shared) = shared_query_cache.as_ref() {
                    QueryCache::new_with_shared(&program.type_interner, shared)
                } else {
                    QueryCache::new(&program.type_interner)
                };
                let (lib_diags, lib_counters, _lib_ds_stats) = check_checker_lib_file(
                    &checker_lib_file_env,
                    lib_idx,
                    &query_cache,
                    Some(Arc::clone(&shared_lib_cache)),
                );
                let mut lib_diags = lib_diags;
                retain_program_induced_lib_diagnostics(&mut lib_diags, &baseline_lib_diagnostics);
                diagnostics.extend(lib_diags);
                request_cache_counters.merge(lib_counters);
                parallel_qc_stats.merge(&query_cache.statistics());
            }
        }
        // PERF: `DefinitionStore::statistics()` walks every entry (and
        // `estimated_size_bytes()` walks again) — only worth paying for
        // when --diagnostics or --extendedDiagnostics is requested.
        let aggregated_ds_stats = if collect_compile_stats {
            program_context
                .shared_definition_store
                .as_ref()
                .map(|store| store.statistics())
                .or(Some(parallel_ds_stats))
        } else {
            None
        };
        (Some(parallel_qc_stats), aggregated_ds_stats)
    } else {
        // --- SEQUENTIAL PATH: Cached build with dependency cascade ---
        // Fallback used only when no shared_definition_store exists (e.g.,
        // tests). Production paths set the shared store unconditionally.
        let sequential_ds_stats = tsz_solver::StoreStatistics::default();

        // Reorder work queue in topological (dependency-first) order so that
        // dependencies are checked before their dependents. This ensures that
        // cached type information and export hashes are available when checking
        // files that import them, improving incremental invalidation accuracy.
        {
            let queue_vec: Vec<usize> = work_queue.iter().copied().collect();
            let ordered = topological_file_order(&queue_vec, &resolved_module_paths);
            work_queue.clear();
            for idx in ordered {
                work_queue.push_back(idx);
            }
        }

        // Process files in the work queue
        while let Some(file_idx) = work_queue.pop_front() {
            let file = &program.files[file_idx];
            let file_path = PathBuf::from(&file.file_name);

            let mut binder = create_binder_from_bound_file_with_augmentations(
                file,
                program,
                file_idx,
                &merged_augmentations,
            );

            // Use cached specifiers from build_resolved_module_maps.
            let module_specifiers = &cached_module_specifiers[file_idx];

            // Bridge multi-file module resolution for ES module imports.
            for (specifier, _, _, _) in module_specifiers {
                if let Some(resolved) = resolve_module_specifier(
                    Path::new(&file.file_name),
                    specifier,
                    options,
                    base_dir,
                    &mut resolution_cache,
                    &program_paths,
                ) {
                    let canonical = normalize_resolved_path(&resolved, options);
                    let canonical = if should_apply_duplicate_package_redirect(&file_path) {
                        package_redirects
                            .get(&canonical)
                            .cloned()
                            .unwrap_or(canonical)
                    } else {
                        canonical
                    };
                    if let Some(&target_idx) = canonical_to_file_idx.get(&canonical) {
                        propagate_module_export_maps(
                            &mut binder,
                            specifier,
                            target_idx,
                            program,
                            &resolved_module_paths,
                        );
                    }
                }
            }
            let cached = cache
                .as_deref_mut()
                .and_then(|cache| cache.type_caches.remove(&file_path));
            let compiler_options = options.checker.clone();
            let mut checker = if let Some(cached) = cached {
                CheckerState::with_cache(
                    &file.arena,
                    &binder,
                    &query_cache,
                    file.file_name.clone(),
                    cached,
                    compiler_options,
                )
            } else {
                CheckerState::new(
                    &file.arena,
                    &binder,
                    &query_cache,
                    file.file_name.clone(),
                    compiler_options,
                )
            };
            checker.ctx.report_unresolved_imports = true;
            program_context.apply_to(&mut checker.ctx);

            // Per-file state that varies across files:
            checker.ctx.set_current_file_idx(file_idx);
            checker.ctx.file_is_esm = program_context
                .file_is_esm_map
                .get(&file.file_name)
                .copied();

            // Use the per-file pre-bucketed map; see the parallel path for the
            // O(N²) → O(1) rationale. `Arc::clone` here is a single atomic
            // increment — no deep copy of the contents.
            let resolved_modules: Arc<rustc_hash::FxHashSet<String>> = resolved_modules_per_file
                .get(file_idx)
                .cloned()
                .unwrap_or_else(|| Arc::new(rustc_hash::FxHashSet::default()));
            checker.ctx.resolved_modules = Some(resolved_modules);
            // TSC suppresses many semantic diagnostics across the whole program when any
            // file has a real syntax parse error; mirror that behavior using the program-level
            // flag so that diagnostics like TS1361/TS1362 do not leak from syntax-error files.
            checker.ctx.has_parse_errors = program_has_real_syntax_errors;
            // Exclude codes that are grammar checks in our parser but are NOT in TSC's
            // parseDiagnostics. TSC uses hasParseDiagnostics() to suppress grammar
            // errors like TS1105/TS1108, so we must match exactly which codes count.
            // Excluded:
            //   TS1009 - Trailing comma (checker grammar error in TSC)
            //   TS1014 - Rest parameter must be last (grammar check, AST is valid)
            //   TS1185 - Merge conflict marker (not a real parse failure)
            //   TS1214 - 'yield' reserved word in strict mode (grammar check, not parse failure)
            //   TS1262 - 'await' reserved word at top level (grammar check, not parse failure)
            //   TS1359 - 'await' reserved word in async context (grammar check, not parse failure)
            checker.ctx.has_syntax_parse_errors = file
                .parse_diagnostics
                .iter()
                .any(|d| !is_non_suppressing_parse_error(d.code));
            checker.ctx.syntax_parse_error_positions = file
                .parse_diagnostics
                .iter()
                .filter(|d| !is_non_suppressing_parse_error(d.code))
                .map(|d| d.start)
                .collect();
            checker.ctx.all_parse_error_positions =
                file.parse_diagnostics.iter().map(|d| d.start).collect();
            // Collect positions of nullable-type syntax errors (`?T`/`T?`) for TS2677
            // widening. Exclude `!T`/`T!` errors which share the same error codes
            // (17019/17020) but should not trigger predicate type widening.
            checker.ctx.nullable_type_parse_error_positions = file
                .parse_diagnostics
                .iter()
                .filter(|d| (d.code == 17019 || d.code == 17020) && d.message.contains("'?'"))
                .map(|d| d.start)
                .collect();
            // Track whether the file has "real" syntax errors (actual parse
            // failures like missing tokens or invalid characters) vs grammar
            // checks (strict mode violations, decorator errors, etc.).
            // Real syntax errors broadly suppress TS2304 to match tsc behavior.
            checker.ctx.has_real_syntax_errors = file
                .parse_diagnostics
                .iter()
                .any(|d| is_real_syntax_error(d.code));
            checker.ctx.has_structural_parse_errors = file
                .parse_diagnostics
                .iter()
                .any(|d| is_structural_parse_error(d.code));
            checker.ctx.real_syntax_error_positions = file
                .parse_diagnostics
                .iter()
                .filter(|d| is_real_syntax_error(d.code))
                .map(|d| d.start)
                .collect();
            let filtered_parse_diagnostics =
                filtered_parse_diagnostics(&file.parse_diagnostics, program_has_real_syntax_errors);
            let is_js = is_js_file(Path::new(&file.file_name));
            let mut file_diagnostics = Vec::new();
            // For JS files, suppress TypeScript-grammar parser diagnostics.
            // tsc's parser is lenient with TypeScript-only syntax in JS files.
            // Keep only parser diagnostics that tsc also emits for JS, and
            // convert TS1162 (optional object member) to TS8009 for methods.
            if is_js {
                let source_text = file
                    .arena
                    .get_source_file_at(file.source_file)
                    .map(|sf| sf.text.as_ref());
                // First, convert specific codes (e.g. TS1162 -> TS8009)
                convert_js_parse_diagnostics_to_ts8xxx(
                    &file.parse_diagnostics,
                    &file.file_name,
                    &mut file_diagnostics,
                    source_text,
                );
                // Then, keep allowed parser diagnostics from the filtered list.
                for parse_diagnostic in &filtered_parse_diagnostics {
                    if is_ts1xxx_allowed_in_js(parse_diagnostic.code) {
                        file_diagnostics.push(parse_diagnostic_to_checker(
                            &file.file_name,
                            parse_diagnostic,
                        ));
                    }
                }
            } else {
                for parse_diagnostic in filtered_parse_diagnostics {
                    file_diagnostics.push(parse_diagnostic_to_checker(
                        &file.file_name,
                        parse_diagnostic,
                    ));
                }
            }
            // skipLibCheck: skip type checking of declaration files (.d.ts, .d.cts, .d.mts)
            if options.skip_lib_check && is_declaration_file(&file.file_name) {
                diagnostics.extend(file_diagnostics);
                request_cache_counters.merge(checker.ctx.request_cache_counters);
                continue;
            }

            // Note: We always run checking for all files (JS and TS).
            // TypeScript reports syntax/semantic errors like TS1210 (strict mode violations)
            // even for JS files without checkJs. Only type-level errors are gated by checkJs.
            if !options.no_check {
                let _check_span =
                    tracing::info_span!("check_file", file = %file.file_name).entered();
                tsz::checker::reset_stack_overflow_flag();
                checker.check_source_file(file.source_file);

                let mut checker_diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
                post_process_checker_diagnostics(
                    &mut checker_diagnostics,
                    file,
                    options,
                    program_has_real_syntax_errors,
                    program_has_unsupported_js_root,
                    has_deprecation_diagnostics,
                );

                file_diagnostics.extend(checker_diagnostics);
            }

            // Inject pre-computed TS7016 diagnostics for CJS require() calls.
            file_diagnostics.extend(per_file_ts7016_diagnostics[file_idx].iter().cloned());

            // Final JS-specific filter: remove any remaining grammar codes that
            // tsc doesn't emit for JS files. Both the parser and checker can
            // produce these codes; this catch-all ensures they don't leak through.
            if is_js {
                file_diagnostics.retain(|d| !is_checker_grammar_code_suppressed_in_js(d.code));
            }

            // Apply @ts-expect-error / @ts-ignore directive suppression.
            // tsc suppresses all diagnostics on the line following such directives
            // and emits TS2578 for unused @ts-expect-error directives.
            if let Some(source) = file.arena.get_source_file_at(file.source_file) {
                apply_ts_directive_suppression(
                    &file.file_name,
                    source.text.as_ref(),
                    &mut file_diagnostics,
                    options.emit_declarations && options.check_js && is_js,
                );
            }

            // Update the cache and check for export signature changes.
            // Uses the unified binder-level ExportSignature (shared with LSP)
            // so body-only/comment-only/private-symbol edits produce the same
            // invalidation decisions in both CLI and LSP.
            let checker_counters = checker.ctx.request_cache_counters;
            // PERF: Skip per-file `definition_store.statistics()`. When
            // `program_context.shared_definition_store` is set, every per-file
            // checker.ctx.definition_store is `Arc::clone` of the SAME shared
            // store — calling .statistics() per file iterates that shared
            // store N times and produces N× inflated counts. The parallel
            // path already documents this same issue at line ~1159 ("summing
            // per-file was both wasted work and N× inflated"). Compute once
            // after the loop instead.

            if let Some(c) = cache.as_deref_mut() {
                let new_sig = compute_export_signature(program, file, file_idx);
                let new_hash = new_sig.0;
                let old_hash = c.export_hashes.get(&file_path).copied();

                c.type_caches
                    .insert(file_path.clone(), checker.extract_cache());
                c.diagnostics
                    .insert(file_path.clone(), file_diagnostics.clone());
                c.export_hashes.insert(file_path.clone(), new_hash);

                if old_hash != Some(new_hash)
                    && let Some(dependents) = c.reverse_dependencies.get(&file_path)
                {
                    for dep_path in dependents {
                        if let Some(&dep_idx) = canonical_to_file_idx.get(dep_path)
                            && checked_files.insert(dep_idx)
                        {
                            work_queue.push_back(dep_idx);
                            c.type_caches.remove(dep_path);
                            c.diagnostics.remove(dep_path);
                        }
                    }
                }
            } else {
                diagnostics.extend(file_diagnostics);
            }
            request_cache_counters.merge(checker_counters);
        }
        // PERF: same gating as the parallel path above — skip the per-lib
        // post-merge recheck loop when there are no user-induced lib
        // augmentations to validate.
        if needs_lib_recheck {
            for lib_idx in 0..checker_libs.files.len() {
                let (lib_diags, lib_counters, _lib_ds_stats) =
                    check_checker_lib_file(&checker_lib_file_env, lib_idx, &query_cache, None);
                let mut lib_diags = lib_diags;
                retain_program_induced_lib_diagnostics(&mut lib_diags, &baseline_lib_diagnostics);
                diagnostics.extend(lib_diags);
                request_cache_counters.merge(lib_counters);
            }
        }
        // Sequential path: single shared QueryCache — capture stats after all files.
        let query_cache_stats = Some(query_cache.statistics());
        // PERF: skip the shared DefinitionStore stats walk unless --diagnostics
        // / --extendedDiagnostics actually consumes them. Matches the parallel
        // path's gating above.
        let aggregated_ds_stats = if collect_compile_stats {
            program_context
                .shared_definition_store
                .as_ref()
                .map(|store| store.statistics())
                .or(Some(sequential_ds_stats))
        } else {
            None
        };
        (query_cache_stats, aggregated_ds_stats)
    };

    // Collect diagnostics from cache for all files
    // This includes both checked files (now in cache) and unchecked files (cached from previous run)
    if let Some(c) = cache.as_deref() {
        for file in &program.files {
            let file_path = PathBuf::from(&file.file_name);
            if let Some(cached_diags) = c.diagnostics.get(&file_path) {
                diagnostics.extend(cached_diags.clone());
            }
        }
    }

    // Cleanup unused entries from the cache
    if let Some(c) = cache {
        c.type_caches.retain(|path, _| used_paths.contains(path));
        c.diagnostics.retain(|path, _| used_paths.contains(path));
        c.export_hashes.retain(|path, _| used_paths.contains(path));
    }

    if !program_has_real_syntax_errors {
        let mut seen: FxHashSet<(String, u32, u32)> = diagnostics
            .iter()
            .map(|diag| (diag.file.clone(), diag.start, diag.code))
            .collect();
        diagnostics.extend(
            tsz::parallel::collect_reexported_module_augmentation_enum_conflict_diagnostics(
                program,
                resolved_module_paths.as_ref(),
            )
            .into_iter()
            .filter(|diag| seen.insert((diag.file.clone(), diag.start, diag.code))),
        );
    }

    diagnostics.extend(detect_missing_tslib_helper_diagnostics(
        program,
        options,
        base_dir,
        &file_is_esm_map,
    ));
    diagnostics.extend(baseline_lib_datetimeformatpart_diagnostics);

    // Compute module dependency graph statistics for --extendedDiagnostics.
    // PERF: Skip the SCC computation entirely when the CLI won't print stats.
    // tarjan_scc + adjacency dedup is O(V+E) and adj allocates a Vec<Vec<usize>>
    // of file_count.
    let module_dep_stats = if collect_compile_stats {
        Some(compute_module_dependency_stats(
            program.files.len(),
            resolved_module_paths.as_ref(),
        ))
    } else {
        None
    };

    CollectDiagnosticsResult {
        diagnostics,
        request_cache_counters,
        query_cache_stats,
        def_store_stats: aggregated_ds_stats,
        module_dep_stats,
    }
}

pub(super) struct CheckFileForParallelContext<'a> {
    file_idx: usize,
    binder: BinderState,
    program: &'a MergedProgram,
    compiler_options: &'a tsz_common::CheckerOptions,
    /// Project-wide shared environment — replaces individual `lib_contexts`, `all_arenas`,
    /// `all_binders`, skeleton indices, `symbol_file_targets`, `resolved_module_paths/errors`,
    /// `is_external_module_by_file`, `file_is_esm_map`, `typescript_dom_replacement_globals`,
    /// and `has_deprecation_diagnostics` fields.
    program_context: &'a tsz::checker::context::ProgramContext,
    /// Per-file pre-bucketed resolved module specifiers (indexed by `file_idx`).
    /// Replaces a previous per-file scan over the program-wide
    /// `resolved_module_specifiers` set, which made each per-file checker
    /// scale with the size of the WHOLE program rather than its own
    /// import count.
    resolved_modules_per_file: &'a Arc<Vec<Arc<rustc_hash::FxHashSet<String>>>>,
    shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>,
    /// Shared cross-file query cache for multi-file projects.
    /// Eliminates redundant type evaluations and relation checks across files.
    shared_query_cache: Option<&'a tsz_solver::SharedQueryCache>,
    no_check: bool,
    check_js: bool,
    /// `true` when `checkJs: false` was explicitly specified in compiler options.
    /// When set, ALL semantic errors are suppressed for JS files, including the
    /// `plainJSErrors` allowlist that would otherwise survive the filter.
    explicit_check_js_false: bool,
    skip_lib_check: bool,
    program_has_real_syntax_errors: bool,
    program_has_unsupported_js_root: bool,
    /// When `false`, per-file `TypeCache` extraction is skipped entirely.
    /// `TypeCache` is used by the emit pipeline (JS / declaration files) and
    /// by incremental cache reuse. For a `--noEmit` run that does not also
    /// request `--declaration`, nothing consumes it, and extracting it for
    /// every one of N files pins several hash maps per file in memory
    /// throughout the whole check (observed at ~10 GB RSS peak on a
    /// 6000-file repo). Set this `false` in that case.
    extract_type_cache: bool,
}

fn collect_no_check_file_diagnostics(
    file: &tsz::parallel::BoundFile,
    options: &ResolvedCompilerOptions,
    program_has_real_syntax_errors: bool,
) -> Vec<Diagnostic> {
    collect_no_check_parse_diagnostics_for_file(
        &file.file_name,
        &file.arena,
        file.source_file,
        &file.parse_diagnostics,
        options,
        program_has_real_syntax_errors,
    )
}

/// Per-file `CheckerContext` configuration extracted from
/// `check_file_for_parallel`. Sets the fields that vary across files in
/// a program — file index, ESM-ness, resolved modules, and the seven
/// parse-diagnostic-derived fields the checker reads to suppress or
/// classify diagnostics in syntax-error files.
///
/// The split between construction and per-file configuration is the
/// seam `PERFORMANCE_PLAN.md` §6 T2.1.B's sequential session-reuse
/// path will plug into: construct the `CheckerContext` once, then
/// repeatedly call this helper, `check_source_file()`, and
/// `reset_for_next_file()` rather than constructing a fresh
/// `CheckerState` per file.
///
/// This commit only does the extraction; the reuse loop itself is a
/// separate sub-PR.
///
/// Pure refactor: the field assignments and their derivations are
/// byte-for-byte identical to the inline version, so default behavior
/// is unchanged.
fn configure_checker_per_file<'a>(
    ctx: &mut tsz::checker::context::CheckerContext<'a>,
    file: &tsz::parallel::BoundFile,
    file_idx: usize,
    program_context: &tsz::checker::context::ProgramContext,
    resolved_modules: Arc<rustc_hash::FxHashSet<String>>,
    program_has_real_syntax_errors: bool,
) {
    ctx.set_current_file_idx(file_idx);
    ctx.file_is_esm = program_context
        .file_is_esm_map
        .get(&file.file_name)
        .copied();
    ctx.resolved_modules = Some(resolved_modules);
    // TSC suppresses many semantic diagnostics across the whole program when any
    // file has a real syntax parse error; mirror that behavior using the program-level
    // flag so that diagnostics like TS1361/TS1362 do not leak from syntax-error files.
    ctx.has_parse_errors = program_has_real_syntax_errors;
    // Exclude grammar checks that don't affect AST structure from
    // has_syntax_parse_errors so we match TSC's hasParseDiagnostics() behavior.
    //   TS1009 - Trailing comma (checker grammar error in TSC)
    //   TS1014 - Rest parameter must be last (grammar check, AST is valid)
    //   TS1185 - Merge conflict marker (not a real parse failure)
    ctx.has_syntax_parse_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| !is_non_suppressing_parse_error(d.code));
    ctx.syntax_parse_error_positions = file
        .parse_diagnostics
        .iter()
        .filter(|d| !is_non_suppressing_parse_error(d.code))
        .map(|d| d.start)
        .collect();
    ctx.all_parse_error_positions = file.parse_diagnostics.iter().map(|d| d.start).collect();
    ctx.nullable_type_parse_error_positions = file
        .parse_diagnostics
        .iter()
        .filter(|d| (d.code == 17019 || d.code == 17020) && d.message.contains("'?'"))
        .map(|d| d.start)
        .collect();
    ctx.has_real_syntax_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| is_real_syntax_error(d.code));
    ctx.has_structural_parse_errors = file
        .parse_diagnostics
        .iter()
        .any(|d| is_structural_parse_error(d.code));
    ctx.real_syntax_error_positions = file
        .parse_diagnostics
        .iter()
        .filter(|d| is_real_syntax_error(d.code))
        .map(|d| d.start)
        .collect();
}

/// Result of checking a single file for the parallel checking path: diagnostics,
/// optional `TypeCache` snapshot, per-file request counters, and solver
/// query-cache / definition-store statistics aggregated by the caller.
pub(super) type CheckFileResult = (
    Vec<Diagnostic>,
    Option<TypeCache>,
    RequestCacheCounters,
    tsz_solver::QueryCacheStatistics,
    tsz_solver::StoreStatistics,
);

/// Check a single file for the parallel checking path.
///
/// This is extracted from the work queue loop so it can be called from rayon's `par_iter`.
/// Each invocation creates its own `CheckerState` (with its own mutable context)
/// and its own `QueryCache` (using `RefCell`/`Cell` for zero-overhead single-threaded caching).
/// The `TypeInterner` is shared across threads via `DashMap` (thread-safe).
/// Run `check_source_file` on a fully-configured `CheckerState`, then
/// post-process and shape the resulting `Vec<Diagnostic>`.
///
/// Extracted from `check_file_for_parallel` so the T2.1.B sequential
/// session-reuse path (`PERFORMANCE_PLAN.md` §6) can reuse the same
/// per-file check pipeline against a `CheckerState` that's been
/// re-targeted at the next file via `CheckerContext::switch_to_file`,
/// instead of constructing a fresh checker per file.
///
/// **Pure refactor**: the body is byte-for-byte the post-
/// `configure_checker_per_file` portion of `check_file_for_parallel`.
/// Default behavior is unchanged because the same function is called
/// in the same order with the same arguments.
///
/// Caller's contract:
///
/// - `checker` has been constructed for `file` and configured via
///   `configure_checker_per_file` (or `switch_to_file` →
///   `configure_checker_per_file`).
/// - `program_context.apply_to(&mut checker.ctx)` has been called.
/// - `checker.ctx.diagnostics` is drained at function entry — anything
///   left over from a prior file is appended to this file's output.
///   In practice this means callers reusing a `CheckerState` across
///   files must have invoked `switch_to_file` (which drains
///   diagnostics via `reset_for_next_file`) before this function.
#[allow(clippy::too_many_arguments)]
fn run_check_on_existing_checker<'a>(
    checker: &mut CheckerState<'a>,
    file: &tsz::parallel::BoundFile,
    compiler_options: &tsz_common::CheckerOptions,
    program_context: &tsz::checker::context::ProgramContext,
    no_check: bool,
    check_js: bool,
    explicit_check_js_false: bool,
    program_has_real_syntax_errors: bool,
    program_has_unsupported_js_root: bool,
) -> Vec<Diagnostic> {
    let filtered_parse_diagnostics =
        filtered_parse_diagnostics(&file.parse_diagnostics, program_has_real_syntax_errors);
    let is_js = is_js_file(Path::new(&file.file_name));

    // For JS files, suppress parser diagnostics. tsc's parser is lenient
    // with TypeScript-only syntax in JS files (it parses but does not emit
    // errors). The checker emits TS8xxx codes instead. Our parser doesn't
    // distinguish JS vs TS, so we suppress parser diagnostics here.
    // Some parser diagnostics are converted to their TS8xxx equivalents.
    // Use raw (unfiltered) diagnostics for conversion.
    let mut file_diagnostics: Vec<Diagnostic> = if is_js {
        let source_text = file
            .arena
            .get_source_file_at(file.source_file)
            .map(|sf| sf.text.as_ref());
        let mut diags = Vec::new();
        convert_js_parse_diagnostics_to_ts8xxx(
            &file.parse_diagnostics,
            &file.file_name,
            &mut diags,
            source_text,
        );
        for parse_diagnostic in &filtered_parse_diagnostics {
            if is_ts1xxx_allowed_in_js(parse_diagnostic.code) {
                diags.push(parse_diagnostic_to_checker(
                    &file.file_name,
                    parse_diagnostic,
                ));
            }
        }
        diags
    } else {
        filtered_parse_diagnostics
            .into_iter()
            .map(|d| parse_diagnostic_to_checker(&file.file_name, d))
            .collect()
    };

    // Note: We always run checking for all files (JS and TS).
    // TypeScript reports syntax/semantic errors like TS1210 (strict mode violations)
    // even for JS files without checkJs. Only type-level errors are gated by checkJs.
    //
    // Under `--noCheck --declaration`, declaration emit still needs the
    // checker's inferred types (return types, contextual property types,
    // etc.) — tsc runs the checker for declaration emit even when
    // `--noCheck` is set (#3733). Run the checker pass when either the
    // user wants normal checking OR we need type information for
    // declaration emit; in the latter case the produced diagnostics are
    // discarded so `--noCheck` still suppresses type errors.
    let run_checker_for_decl_emit = no_check && compiler_options.emit_declarations;
    if !no_check || run_checker_for_decl_emit {
        tsz::checker::reset_stack_overflow_flag();
        checker.check_source_file(file.source_file);
        let mut checker_diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
        let effective_options = ResolvedCompilerOptions {
            check_js,
            explicit_check_js_false,
            ..ResolvedCompilerOptions::default()
        };
        post_process_checker_diagnostics(
            &mut checker_diagnostics,
            file,
            &effective_options,
            program_has_real_syntax_errors,
            program_has_unsupported_js_root,
            program_context.has_deprecation_diagnostics,
        );

        if !no_check {
            file_diagnostics.extend(checker_diagnostics);
        } else if compiler_options.isolated_declarations {
            // `--noCheck` suppresses type errors, but the
            // `--isolatedDeclarations` family (TS9007–TS9039) gates
            // declaration emission and tsc still surfaces those codes
            // (#3709). Keep them, drop everything else.
            file_diagnostics.extend(
                checker_diagnostics
                    .into_iter()
                    .filter(|d| matches!(d.code, 9007..=9039)),
            );
        }
    }

    // Final JS-specific filter: remove any remaining grammar codes that
    // tsc doesn't emit for JS files.
    if is_js {
        file_diagnostics.retain(|d| !is_checker_grammar_code_suppressed_in_js(d.code));
    }

    // Apply @ts-expect-error / @ts-ignore directive suppression.
    if let Some(source) = file.arena.get_source_file_at(file.source_file) {
        apply_ts_directive_suppression(
            &file.file_name,
            source.text.as_ref(),
            &mut file_diagnostics,
            compiler_options.emit_declarations && check_js && is_js,
        );
    }

    file_diagnostics
}

pub(super) fn check_file_for_parallel<'a>(
    context: CheckFileForParallelContext<'a>,
) -> CheckFileResult {
    let CheckFileForParallelContext {
        file_idx,
        binder,
        program,
        compiler_options,
        program_context,
        resolved_modules_per_file,
        shared_lib_cache,
        shared_query_cache,
        no_check,
        check_js,
        explicit_check_js_false,
        skip_lib_check,
        program_has_real_syntax_errors,
        program_has_unsupported_js_root,
        extract_type_cache,
    } = context;
    let file = &program.files[file_idx];
    // skipLibCheck: skip type checking of declaration files (.d.ts, .d.cts, .d.mts)
    if skip_lib_check && is_declaration_file(&file.file_name) {
        return (
            Vec::new(),
            None,
            RequestCacheCounters::default(),
            tsz_solver::QueryCacheStatistics::default(),
            tsz_solver::StoreStatistics::default(),
        );
    }

    // Create a per-thread QueryCache (uses RefCell/Cell, no atomic overhead).
    // For multi-file projects, use shared L2 cache to avoid redundant computation.
    let query_cache = if let Some(shared) = shared_query_cache {
        QueryCache::new_with_shared(&program.type_interner, shared)
    } else {
        QueryCache::new(&program.type_interner)
    };

    // Use the pre-bucketed `resolved_modules_per_file[file_idx]` instead of
    // re-filtering the program-wide cross-file set per file. The bucketed
    // version is built once in `collect_diagnostics` and shared via `Arc`.
    // Per-file `Arc::clone` is a single atomic increment — no deep copy of
    // the `FxHashSet<String>` contents. Saves ~120K string clones on the
    // 6086-file large-ts-repo fixture.
    let resolved_modules: Arc<FxHashSet<String>> = resolved_modules_per_file
        .get(file_idx)
        .cloned()
        .unwrap_or_else(|| Arc::new(FxHashSet::default()));

    // apply_to (below) installs the project-wide shared DefinitionStore and
    // warms the per-file caches from it. Use the deferred constructor so we
    // don't build a throwaway per-file store first — that work showed up in
    // profiles as a non-trivial fraction of total CPU on large projects, all
    // of it overwritten moments later.
    let mut checker = CheckerState::with_options_deferred_def_store(
        &file.arena,
        &binder,
        &query_cache,
        file.file_name.clone(),
        compiler_options,
    );
    checker.ctx.report_unresolved_imports = true;
    checker.ctx.shared_lib_type_cache = Some(shared_lib_cache);

    // Apply all project-level shared state in one call. This installs the
    // shared DefinitionStore and runs warm_local_caches_from_shared_store().
    program_context.apply_to(&mut checker.ctx);

    // Per-file `CheckerContext` configuration. Extracted into a helper
    // to seam construction from per-file configuration; T2.1.B's
    // sequential session-reuse path will reuse this entry point.
    configure_checker_per_file(
        &mut checker.ctx,
        file,
        file_idx,
        program_context,
        resolved_modules,
        program_has_real_syntax_errors,
    );
    let file_diagnostics = run_check_on_existing_checker(
        &mut checker,
        file,
        compiler_options,
        program_context,
        no_check,
        check_js,
        explicit_check_js_false,
        program_has_real_syntax_errors,
        program_has_unsupported_js_root,
    );

    let checker_counters = checker.ctx.request_cache_counters;
    let qc_stats = query_cache.statistics();
    // Skip per-file DefinitionStore statistics: in the parallel path all
    // checkers share the same store, so every worker would report the same
    // numbers and the aggregator was summing them N times (both wasted work
    // and inflated counts). The aggregator computes stats once on the
    // shared store after the work loop completes.
    let ds_stats = tsz_solver::StoreStatistics::default();
    let type_cache = if extract_type_cache {
        Some(checker.extract_cache())
    } else {
        None
    };
    (
        file_diagnostics,
        type_cache,
        checker_counters,
        qc_stats,
        ds_stats,
    )
}

/// Sequential session-reuse path for T2.1.B (`PERFORMANCE_PLAN.md` §6
/// PR table item T2.1.B: "Add a sequential session-reuse path behind
/// a flag").
///
/// Differences from the default `work_items.iter().map(check_file_for_parallel).collect()`
/// path:
///
/// 1. **One `CheckerState` for the entire loop** (vs. one per file).
///    Constructed lazily on the first non-skip-lib-check file so an
///    all-declaration-file `work_items` doesn't pay setup cost.
/// 2. **One `QueryCache` for the entire loop** (vs. one per file).
///    The shared L2 path (`shared_query_cache`) already shared a
///    cache across files when present; this path also reuses the
///    primary `QueryCache` across files when `shared_query_cache` is
///    `None`.
/// 3. **`program_context.apply_to` runs once** (vs. once per file).
///    The `apply_to` work — Arc-cloning shared program-level state
///    into `ctx`, warming the local caches from the shared
///    `DefinitionStore` — is identical across files and only
///    needs to land once. Subsequent files inherit it through the
///    same `ctx`.
/// 4. **Pre-built `Vec<BinderState>`** holds every file's binder for
///    the duration of the loop, satisfying `CheckerState`'s `&'a
///    BinderState` lifetime requirement. The fresh-per-file path
///    drops the binder at each iteration's end; this path holds
///    them all so the next `switch_to_file` call has a valid
///    `&BinderState` to swap to.
///
/// Per-file work that still happens N times:
/// - `configure_checker_per_file` (file-local config: `file_idx`,
///   `resolved_modules`, parse-error positions, etc.)
/// - `CheckerContext::switch_to_file` (drains file-local caches,
///   swaps `arena`/`binder`/`file_name`/`file_idx`)
/// - The actual `check_source_file` work and diagnostic
///   post-processing (via `run_check_on_existing_checker`)
///
/// Caller's contract: enabled by default for sequential no-emit runs;
/// `TSZ_DISABLE_FILE_SESSION_REUSE=1` opts out. The flag-off path
/// goes through `check_file_for_parallel` per file unchanged.
///
/// **Correctness gate**: this path must produce byte-identical
/// diagnostics to the flag-off path under any conformance fixture,
/// or it is wrong (`PERFORMANCE_PLAN.md` §6 T2.1.B `DoD` line). If a
/// future change introduces a divergence, the responsible change is
/// the one to fix, not the flag — the flag exists to *measure* the
/// allocation savings, not to gate behavior changes.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
fn check_files_sequentially_with_reuse<F>(
    work_items: &[usize],
    program: &MergedProgram,
    compiler_options: &tsz_common::CheckerOptions,
    program_context: &tsz::checker::context::ProgramContext,
    resolved_modules_per_file: &Arc<Vec<Arc<rustc_hash::FxHashSet<String>>>>,
    shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>,
    shared_query_cache: Option<&tsz_solver::SharedQueryCache>,
    no_check: bool,
    check_js: bool,
    explicit_check_js_false: bool,
    skip_lib_check: bool,
    program_has_real_syntax_errors: bool,
    program_has_unsupported_js_root: bool,
    extract_type_cache: bool,
    build_checker_binder: F,
) -> Vec<CheckFileResult>
where
    F: Fn(usize) -> tsz_binder::BinderState,
{
    // Pre-build every binder via the caller-provided closure. Each
    // file's `CheckerContext::binder` is a `&'a BinderState`, so the
    // binders must outlive the `CheckerState` we construct below;
    // collecting into a `Vec` owned by this function satisfies that.
    // The closure form lets the caller hold the module-resolution
    // tables (`cached_module_specifiers`, `resolved_module_paths`,
    // `merged_augmentations`) in its own scope without threading them
    // through this function's signature.
    let binders: Vec<tsz_binder::BinderState> = work_items
        .iter()
        .map(|&file_idx| build_checker_binder(file_idx))
        .collect();

    // One `QueryCache` for the whole loop. Mirrors the per-file
    // construction in `check_file_for_parallel`, but built once.
    let query_cache = if let Some(shared) = shared_query_cache {
        QueryCache::new_with_shared(&program.type_interner, shared)
    } else {
        QueryCache::new(&program.type_interner)
    };

    let mut results: Vec<CheckFileResult> = Vec::with_capacity(work_items.len());
    let mut checker: Option<CheckerState> = None;

    for (loop_idx, &file_idx) in work_items.iter().enumerate() {
        let file = &program.files[file_idx];

        // skipLibCheck: skip type checking of declaration files. Same
        // contract as `check_file_for_parallel`'s early-return; we
        // emit an empty result and do *not* touch the shared
        // `CheckerState` for this file.
        if skip_lib_check && is_declaration_file(&file.file_name) {
            results.push((
                Vec::new(),
                None,
                RequestCacheCounters::default(),
                tsz_solver::QueryCacheStatistics::default(),
                tsz_solver::StoreStatistics::default(),
            ));
            continue;
        }

        let resolved_modules: Arc<rustc_hash::FxHashSet<String>> = resolved_modules_per_file
            .get(file_idx)
            .cloned()
            .unwrap_or_else(|| Arc::new(rustc_hash::FxHashSet::default()));

        // Lazy construction on the first non-skipped file. After this,
        // subsequent iterations use `switch_to_file` to re-target the
        // same `CheckerState` at the next file.
        if checker.is_none() {
            let mut state = CheckerState::with_options_deferred_def_store(
                &file.arena,
                &binders[loop_idx],
                &query_cache,
                file.file_name.clone(),
                compiler_options,
            );
            state.ctx.report_unresolved_imports = true;
            state.ctx.shared_lib_type_cache = Some(Arc::clone(&shared_lib_cache));
            // `apply_to` is the expensive setup we're amortising:
            // shared `DefinitionStore`, shared global indices,
            // resolved-module maps, file-is-ESM map, etc. Running it
            // once vs. N-times is the headline win for this path.
            program_context.apply_to(&mut state.ctx);
            checker = Some(state);
        } else if let Some(ref mut state) = checker {
            state.ctx.switch_to_file(
                &file.arena,
                &binders[loop_idx],
                file.file_name.clone(),
                file_idx,
            );
        }

        let state = checker.as_mut().expect("checker constructed above");
        configure_checker_per_file(
            &mut state.ctx,
            file,
            file_idx,
            program_context,
            resolved_modules,
            program_has_real_syntax_errors,
        );

        let file_diagnostics = run_check_on_existing_checker(
            state,
            file,
            compiler_options,
            program_context,
            no_check,
            check_js,
            explicit_check_js_false,
            program_has_real_syntax_errors,
            program_has_unsupported_js_root,
        );

        let checker_counters = state.ctx.request_cache_counters;
        // `QueryCache::statistics()` is cumulative over the whole loop
        // because we reuse the same cache. The aggregator merges per-
        // file stats; emitting cumulative numbers N times would inflate
        // them. Emit them once on the last iteration to keep the
        // aggregator's invariant: sum of per-file QC stats == final
        // cumulative QC stats.
        let qc_stats = if loop_idx + 1 == work_items.len() {
            query_cache.statistics()
        } else {
            tsz_solver::QueryCacheStatistics::default()
        };
        let ds_stats = tsz_solver::StoreStatistics::default();
        // The reuse path is gated on `!extract_type_cache` at the
        // call site; this loop never observes `extract_type_cache=true`,
        // so we always emit `None` for the per-file `TypeCache` slot.
        // See the call site in the sequential-branch dispatch for
        // the rationale.
        let type_cache = None;
        let _ = extract_type_cache;

        results.push((
            file_diagnostics,
            type_cache,
            checker_counters,
            qc_stats,
            ds_stats,
        ));
    }

    results
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
fn check_files_in_parallel_chunks_with_reuse<F>(
    work_items: &[usize],
    program: &MergedProgram,
    compiler_options: &tsz_common::CheckerOptions,
    program_context: &tsz::checker::context::ProgramContext,
    resolved_modules_per_file: &Arc<Vec<Arc<rustc_hash::FxHashSet<String>>>>,
    shared_lib_cache: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>,
    shared_query_cache: Option<&tsz_solver::SharedQueryCache>,
    no_check: bool,
    check_js: bool,
    explicit_check_js_false: bool,
    skip_lib_check: bool,
    program_has_real_syntax_errors: bool,
    program_has_unsupported_js_root: bool,
    extract_type_cache: bool,
    chunk_size: usize,
    build_checker_binder: &F,
) -> Vec<CheckFileResult>
where
    F: Fn(usize) -> tsz_binder::BinderState + Sync,
{
    use rayon::iter::ParallelIterator;
    use rayon::slice::ParallelSlice;

    debug_assert!(!extract_type_cache);
    let chunk_size = chunk_size.max(1);
    work_items
        .par_chunks(chunk_size)
        .map(|chunk| {
            check_files_sequentially_with_reuse(
                chunk,
                program,
                compiler_options,
                program_context,
                resolved_modules_per_file,
                Arc::clone(&shared_lib_cache),
                shared_query_cache,
                no_check,
                check_js,
                explicit_check_js_false,
                skip_lib_check,
                program_has_real_syntax_errors,
                program_has_unsupported_js_root,
                extract_type_cache,
                build_checker_binder,
            )
        })
        .collect::<Vec<_>>()
        .into_iter()
        .flatten()
        .collect()
}

struct CheckerLibFileCheckEnv<'a> {
    program: &'a MergedProgram,
    options: &'a ResolvedCompilerOptions,
    checker_libs: &'a CheckerLibSet,
    affected_interfaces: &'a FxHashSet<String>,
    extension_interfaces: &'a FxHashSet<String>,
    merged_augmentations: &'a MergedAugmentations,
    program_context: &'a tsz::checker::context::ProgramContext,
    program_has_real_syntax_errors: bool,
    program_has_unsupported_js_root: bool,
}

fn check_checker_lib_file(
    env: &CheckerLibFileCheckEnv<'_>,
    lib_idx: usize,
    query_cache: &QueryCache,
    shared_lib_cache: Option<Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>>,
) -> (
    Vec<Diagnostic>,
    RequestCacheCounters,
    tsz_solver::StoreStatistics,
) {
    check_checker_lib_file_for_interfaces(
        env,
        lib_idx,
        env.affected_interfaces,
        env.extension_interfaces,
        query_cache,
        shared_lib_cache,
    )
}

fn check_checker_lib_file_for_interfaces(
    env: &CheckerLibFileCheckEnv<'_>,
    lib_idx: usize,
    interface_names: &FxHashSet<String>,
    extension_interfaces: &FxHashSet<String>,
    query_cache: &QueryCache,
    shared_lib_cache: Option<Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>>,
) -> (
    Vec<Diagnostic>,
    RequestCacheCounters,
    tsz_solver::StoreStatistics,
) {
    let program = env.program;
    let options = env.options;
    let checker_libs = env.checker_libs;
    let lib_file = &checker_libs.files[lib_idx];
    if should_skip_type_checking_for_file(&lib_file.file_name, options, true) {
        return (
            Vec::new(),
            RequestCacheCounters::default(),
            tsz_solver::StoreStatistics::default(),
        );
    }

    let lib_bound_file =
        build_lib_bound_file_for_interface_checks(program, lib_file, interface_names);
    let binder = create_binder_from_bound_file_with_augmentations(
        &lib_bound_file,
        program,
        program.files.len(),
        env.merged_augmentations,
    );

    let mut checker = CheckerState::with_options(
        &lib_bound_file.arena,
        &binder,
        query_cache,
        lib_bound_file.file_name.clone(),
        &options.checker,
    );
    env.program_context.apply_to(&mut checker.ctx);
    if let Some(shared_lib_cache) = shared_lib_cache {
        checker.ctx.shared_lib_type_cache = Some(shared_lib_cache);
    }

    // The current lib file is already the primary binder for this checker. Exclude it from the
    // fallback lib contexts to avoid duplicate lib declarations during cross-file lookup while
    // still preserving the total count used for capability checks.
    let other_lib_contexts: Vec<LibContext> = checker_libs
        .contexts
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != lib_idx)
        .map(|(_, ctx)| ctx.clone())
        .collect();
    checker.ctx.set_lib_contexts(other_lib_contexts);
    checker
        .ctx
        .set_actual_lib_file_count(checker_libs.contexts.len());
    checker.prime_boxed_types();

    // Lib files have no parser diagnostics in the checker pass, but semantic diagnostics from
    // them should still respect tsc's global syntax-error suppression policy.
    checker.ctx.has_parse_errors = env.program_has_real_syntax_errors;
    tsz::checker::reset_stack_overflow_flag();
    checker.check_source_file_interfaces_only_filtered_post_merge(
        lib_bound_file.source_file,
        interface_names,
        extension_interfaces,
    );

    let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
    if env.program_has_real_syntax_errors {
        diagnostics
            .retain(|diag| keep_checker_diagnostic_when_program_has_real_syntax_errors(diag.code));
    }
    if env.program_has_unsupported_js_root && !env.program_has_real_syntax_errors {
        diagnostics
            .retain(|diag| keep_checker_diagnostic_when_program_has_real_syntax_errors(diag.code));
    }
    diagnostics.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));
    diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

    // PERF: All callers Arc::clone the same shared DefinitionStore; the
    // aggregator computes stats once on the shared store after the loop.
    // See `check_file_for_parallel` for the same rationale.
    (
        diagnostics,
        checker.ctx.request_cache_counters,
        tsz_solver::StoreStatistics::default(),
    )
}

fn check_checker_lib_file_baseline(
    program_context: &tsz::checker::context::ProgramContext,
    options: &ResolvedCompilerOptions,
    checker_libs: &CheckerLibSet,
    lib_idx: usize,
    affected_interfaces: &FxHashSet<String>,
    extension_interfaces: &FxHashSet<String>,
    query_cache: &QueryCache,
) -> (
    Vec<Diagnostic>,
    RequestCacheCounters,
    tsz_solver::StoreStatistics,
) {
    let lib_file = &checker_libs.files[lib_idx];
    if should_skip_type_checking_for_file(&lib_file.file_name, options, true) {
        return (
            Vec::new(),
            RequestCacheCounters::default(),
            tsz_solver::StoreStatistics::default(),
        );
    }

    let mut checker = CheckerState::with_options(
        &lib_file.arena,
        lib_file.binder.as_ref(),
        query_cache,
        lib_file.file_name.clone(),
        &options.checker,
    );
    program_context.apply_to(&mut checker.ctx);
    let other_lib_contexts: Vec<LibContext> = checker_libs
        .contexts
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != lib_idx)
        .map(|(_, ctx)| ctx.clone())
        .collect();
    checker.ctx.set_lib_contexts(other_lib_contexts);
    checker
        .ctx
        .set_actual_lib_file_count(checker_libs.contexts.len());
    checker.prime_boxed_types();
    tsz::checker::reset_stack_overflow_flag();
    checker.check_source_file_interfaces_only_filtered_post_merge(
        lib_file.root_index,
        affected_interfaces,
        extension_interfaces,
    );

    let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
    diagnostics.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));
    diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

    // PERF: Same as `check_checker_lib_file` — callers ignore stats and the
    // aggregator computes them once on the shared store.
    (
        diagnostics,
        checker.ctx.request_cache_counters,
        tsz_solver::StoreStatistics::default(),
    )
}

fn collect_lib_interface_node_symbols(
    arena: &NodeArena,
    statements: &[NodeIndex],
    globals: &SymbolTable,
    fallback_node_symbols: &FxHashMap<u32, tsz::binder::SymbolId>,
    affected_interfaces: &FxHashSet<String>,
    node_symbols: &mut FxHashMap<u32, tsz::binder::SymbolId>,
) {
    for &stmt_idx in statements {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };

        if stmt_node.kind == tsz::parser::syntax_kind_ext::INTERFACE_DECLARATION {
            if let Some(interface) = arena.get_interface(stmt_node)
                && let Some(name) = arena.get_identifier_at(interface.name)
                && affected_interfaces.contains(&name.escaped_text)
                && let Some(sym_id) = globals
                    .get(&name.escaped_text)
                    .or_else(|| fallback_node_symbols.get(&stmt_idx.0).copied())
            {
                node_symbols.insert(stmt_idx.0, sym_id);
                node_symbols.insert(interface.name.0, sym_id);
                if let Some(heritage_clauses) = &interface.heritage_clauses {
                    for &clause_idx in &heritage_clauses.nodes {
                        let Some(clause_node) = arena.get(clause_idx) else {
                            continue;
                        };
                        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                            continue;
                        };
                        if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                            continue;
                        }
                        for &type_idx in &heritage.types.nodes {
                            let Some(type_node) = arena.get(type_idx) else {
                                continue;
                            };
                            let expr_idx = if let Some(expr_type_args) =
                                arena.get_expr_type_args(type_node)
                            {
                                expr_type_args.expression
                            } else if type_node.kind == tsz::parser::syntax_kind_ext::TYPE_REFERENCE
                            {
                                arena
                                    .get_type_ref(type_node)
                                    .map_or(type_idx, |type_ref| type_ref.type_name)
                            } else {
                                type_idx
                            };
                            if let Some(base_name) = entity_name_text_in_arena(arena, expr_idx)
                                && let Some(base_sym_id) = globals
                                    .get(&base_name)
                                    .or_else(|| fallback_node_symbols.get(&expr_idx.0).copied())
                            {
                                node_symbols.insert(expr_idx.0, base_sym_id);
                            }
                        }
                    }
                }
            }
            continue;
        }

        if stmt_node.kind != tsz::parser::syntax_kind_ext::MODULE_DECLARATION {
            continue;
        }

        let Some(module_decl) = arena.get_module(stmt_node) else {
            continue;
        };
        if module_decl.body.is_none() {
            continue;
        }
        let Some(body_node) = arena.get(module_decl.body) else {
            continue;
        };
        if body_node.kind != tsz::parser::syntax_kind_ext::MODULE_BLOCK {
            continue;
        }
        let Some(block) = arena.get_module_block(body_node) else {
            continue;
        };
        let Some(inner) = &block.statements else {
            continue;
        };
        collect_lib_interface_node_symbols(
            arena,
            &inner.nodes,
            globals,
            fallback_node_symbols,
            affected_interfaces,
            node_symbols,
        );
    }
}

fn interface_name_text(arena: &NodeArena, stmt_idx: NodeIndex) -> Option<String> {
    let node = arena.get(stmt_idx)?;
    let interface = arena.get_interface(node)?;
    let ident = arena.get_identifier_at(interface.name)?;
    Some(ident.escaped_text.clone())
}

fn entity_name_text_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == tsz::parser::syntax_kind_ext::TYPE_REFERENCE
        && let Some(type_ref) = arena.get_type_ref(node)
    {
        return entity_name_text_in_arena(arena, type_ref.type_name);
    }
    if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
        return arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone());
    }
    if node.kind == tsz::parser::syntax_kind_ext::QUALIFIED_NAME {
        let qn = arena.get_qualified_name(node)?;
        let left = entity_name_text_in_arena(arena, qn.left)?;
        let right = entity_name_text_in_arena(arena, qn.right)?;
        return Some(format!("{left}.{right}"));
    }
    if node.kind == tsz::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        && let Some(access) = arena.get_access_expr(node)
    {
        let left = entity_name_text_in_arena(arena, access.expression)?;
        let right = arena
            .get(access.name_or_argument)
            .and_then(|right_node| arena.get_identifier(right_node))?;
        return Some(format!("{left}.{}", right.escaped_text));
    }
    None
}

fn collect_direct_base_names(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
) -> Vec<String> {
    let Some(heritage_clauses) = &interface.heritage_clauses else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for &clause_idx in &heritage_clauses.nodes {
        let Some(clause_node) = arena.get(clause_idx) else {
            continue;
        };
        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
            continue;
        };
        if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
            continue;
        }
        for &type_idx in &heritage.types.nodes {
            let Some(type_node) = arena.get(type_idx) else {
                continue;
            };
            let expr_idx = if let Some(expr_type_args) = arena.get_expr_type_args(type_node) {
                expr_type_args.expression
            } else if type_node.kind == tsz::parser::syntax_kind_ext::TYPE_REFERENCE {
                arena
                    .get_type_ref(type_node)
                    .map_or(type_idx, |type_ref| type_ref.type_name)
            } else {
                type_idx
            };
            if let Some(name) = entity_name_text_in_arena(arena, expr_idx) {
                names.push(name);
            }
        }
    }
    names
}

fn interface_declaration_has_merge_surface(arena: &NodeArena, stmt_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(stmt_idx) else {
        return false;
    };
    let Some(interface) = arena.get_interface(node) else {
        return false;
    };

    !interface.members.nodes.is_empty()
        || interface
            .type_parameters
            .as_ref()
            .is_some_and(|type_params| !type_params.nodes.is_empty())
        || interface
            .heritage_clauses
            .as_ref()
            .is_some_and(|heritage| !heritage.nodes.is_empty())
}

fn collect_user_global_interface_seeds(program: &MergedProgram) -> FxHashSet<String> {
    let mut seeds = FxHashSet::default();

    for file in &program.files {
        if !file.is_external_module
            && let Some(source_file) = file.arena.get_source_file_at(file.source_file)
        {
            for &stmt_idx in &source_file.statements.nodes {
                if interface_declaration_has_merge_surface(file.arena.as_ref(), stmt_idx)
                    && let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx)
                {
                    seeds.insert(name);
                }
            }
        }

        for (name, augmentations) in file.global_augmentations.iter() {
            let affects_interface = augmentations.iter().any(|augmentation| {
                if (augmentation.flags & tsz::binder::symbol_flags::INTERFACE) == 0 {
                    return true;
                }
                let arena = augmentation
                    .arena
                    .as_deref()
                    .unwrap_or_else(|| file.arena.as_ref());
                interface_declaration_has_merge_surface(arena, augmentation.node)
            });
            if affects_interface {
                seeds.insert(name.clone());
            }
        }
    }

    seeds
}

fn member_name_text(arena: &NodeArena, member_idx: NodeIndex) -> Option<String> {
    let member_node = arena.get(member_idx)?;
    if let Some(sig) = arena.get_signature(member_node) {
        return arena
            .get(sig.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone());
    }
    if let Some(accessor) = arena.get_accessor(member_node) {
        return arena
            .get(accessor.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone());
    }
    None
}

fn collect_user_global_interface_member_names(program: &MergedProgram) -> FxHashSet<String> {
    let mut member_names = FxHashSet::default();

    for file in &program.files {
        if file.is_external_module {
            continue;
        }
        let Some(source_file) = file.arena.get_source_file_at(file.source_file) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = file.arena.get_interface(stmt_node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                if let Some(name) = member_name_text(file.arena.as_ref(), member_idx) {
                    member_names.insert(name);
                }
            }
        }
    }

    member_names
}

fn add_user_global_interface_declaration_arenas(
    program: &MergedProgram,
    declaration_arenas: &mut tsz::binder::state::DeclarationArenaMap,
) {
    for file in &program.files {
        if file.is_external_module {
            continue;
        }
        let Some(source_file) = file.arena.get_source_file_at(file.source_file) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let Some(sym_id) = program.globals.get(&name) else {
                continue;
            };
            let target = declaration_arenas.entry((sym_id, stmt_idx)).or_default();
            if !target.iter().any(|arena| Arc::ptr_eq(arena, &file.arena)) {
                target.push(Arc::clone(&file.arena));
            }
        }
    }
}

fn type_node_contains_tag_name_map_indexed_access(
    arena: &NodeArena,
    type_idx: NodeIndex,
    fuel: &mut u32,
) -> bool {
    if type_idx == NodeIndex::NONE || *fuel == 0 {
        return false;
    }
    *fuel -= 1;

    let Some(node) = arena.get(type_idx) else {
        return false;
    };
    if node.kind == tsz::parser::syntax_kind_ext::INDEXED_ACCESS_TYPE {
        return arena
            .get_indexed_access_type(node)
            .and_then(|indexed| entity_name_text_in_arena(arena, indexed.object_type))
            .is_some_and(|name| name.contains("TagNameMap"));
    }

    if let Some(type_ref) = arena.get_type_ref(node) {
        return type_ref.type_arguments.as_ref().is_some_and(|args| {
            args.nodes
                .iter()
                .any(|&arg| type_node_contains_tag_name_map_indexed_access(arena, arg, fuel))
        });
    }
    if let Some(composite) = arena.get_composite_type(node) {
        return composite
            .types
            .nodes
            .iter()
            .any(|&ty| type_node_contains_tag_name_map_indexed_access(arena, ty, fuel));
    }
    if let Some(array) = arena.get_array_type(node) {
        return type_node_contains_tag_name_map_indexed_access(arena, array.element_type, fuel);
    }
    if let Some(wrapped) = arena.get_wrapped_type(node) {
        return type_node_contains_tag_name_map_indexed_access(arena, wrapped.type_node, fuel);
    }
    if let Some(type_operator) = arena.get_type_operator(node) {
        return type_node_contains_tag_name_map_indexed_access(
            arena,
            type_operator.type_node,
            fuel,
        );
    }
    if let Some(function_type) = arena.get_function_type(node) {
        if type_node_contains_tag_name_map_indexed_access(
            arena,
            function_type.type_annotation,
            fuel,
        ) {
            return true;
        }
        for &param_idx in &function_type.parameters.nodes {
            let Some(param_node) = arena.get(param_idx) else {
                continue;
            };
            let Some(param) = arena.get_parameter(param_node) else {
                continue;
            };
            if type_node_contains_tag_name_map_indexed_access(arena, param.type_annotation, fuel) {
                return true;
            }
        }
    }
    if let Some(conditional) = arena.get_conditional_type(node) {
        return [
            conditional.check_type,
            conditional.extends_type,
            conditional.true_type,
            conditional.false_type,
        ]
        .into_iter()
        .any(|ty| type_node_contains_tag_name_map_indexed_access(arena, ty, fuel));
    }

    false
}

fn interface_declares_member_named(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
    member_names: &FxHashSet<String>,
) -> bool {
    !member_names.is_empty()
        && interface.members.nodes.iter().any(|&member_idx| {
            member_name_text(arena, member_idx).is_some_and(|name| member_names.contains(&name))
        })
}

fn interface_declares_index_signature(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
) -> bool {
    interface.members.nodes.iter().any(|&member_idx| {
        arena
            .get(member_idx)
            .is_some_and(|member| member.kind == tsz::parser::syntax_kind_ext::INDEX_SIGNATURE)
    })
}

fn collect_user_global_interfaces_with_index_signatures(
    program: &MergedProgram,
) -> FxHashSet<String> {
    let mut names = FxHashSet::default();

    for file in &program.files {
        if !file.is_external_module
            && let Some(source_file) = file.arena.get_source_file_at(file.source_file)
        {
            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = file.arena.get(stmt_idx) else {
                    continue;
                };
                let Some(interface) = file.arena.get_interface(stmt_node) else {
                    continue;
                };
                if interface_declares_index_signature(file.arena.as_ref(), interface)
                    && let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx)
                {
                    names.insert(name);
                }
            }
        }

        for (name, augmentations) in file.global_augmentations.iter() {
            if augmentations.iter().any(|augmentation| {
                let arena = augmentation
                    .arena
                    .as_deref()
                    .unwrap_or_else(|| file.arena.as_ref());
                arena
                    .get(augmentation.node)
                    .and_then(|node| arena.get_interface(node))
                    .is_some_and(|interface| interface_declares_index_signature(arena, interface))
            }) {
                names.insert(name.clone());
            }
        }
    }

    names
}

fn interface_has_indexed_access_member_type(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
) -> bool {
    for &member_idx in &interface.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        if let Some(sig) = arena.get_signature(member_node) {
            let mut fuel = 256;
            if type_node_contains_tag_name_map_indexed_access(arena, sig.type_annotation, &mut fuel)
            {
                return true;
            }
            for &param_idx in sig.parameters.as_ref().map_or(&[][..], |p| &p.nodes) {
                let Some(param_node) = arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = arena.get_parameter(param_node) else {
                    continue;
                };
                let mut fuel = 256;
                if type_node_contains_tag_name_map_indexed_access(
                    arena,
                    param.type_annotation,
                    &mut fuel,
                ) {
                    return true;
                }
            }
        }
        if let Some(accessor) = arena.get_accessor(member_node) {
            let mut fuel = 256;
            if type_node_contains_tag_name_map_indexed_access(
                arena,
                accessor.type_annotation,
                &mut fuel,
            ) {
                return true;
            }
        }
    }

    false
}

fn affected_lib_interface_names(
    program: &MergedProgram,
    checker_libs: &CheckerLibSet,
) -> FxHashSet<String> {
    let seed_interfaces = collect_user_global_interface_seeds(program);
    let index_signature_seed_interfaces =
        collect_user_global_interfaces_with_index_signatures(program);
    let mut affected = seed_interfaces.clone();
    let user_member_names = collect_user_global_interface_member_names(program);
    let mut inheritance_graph: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();

    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let bases = collect_direct_base_names(lib.arena.as_ref(), interface);
            inheritance_graph.entry(name).or_default().extend(bases);
        }
    }

    let mut index_signature_affected = index_signature_seed_interfaces;
    let mut changed = true;
    while changed {
        changed = false;
        for (name, bases) in &inheritance_graph {
            if index_signature_affected.contains(name) {
                continue;
            }
            if bases
                .iter()
                .any(|base| index_signature_affected.contains(base))
            {
                changed = index_signature_affected.insert(name.clone());
            }
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for (name, bases) in &inheritance_graph {
            if affected.contains(name) {
                continue;
            }
            if bases.iter().any(|base| affected.contains(base)) {
                changed = affected.insert(name.clone());
            }
        }
    }

    let mut relevant = FxHashSet::default();
    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            if !affected.contains(&name) {
                continue;
            }
            if (index_signature_affected.contains(&name)
                && interface_declares_index_signature(lib.arena.as_ref(), interface))
                || interface_declares_member_named(
                    lib.arena.as_ref(),
                    interface,
                    &user_member_names,
                )
                || interface_has_indexed_access_member_type(lib.arena.as_ref(), interface)
            {
                relevant.insert(name);
            }
        }
    }

    relevant.extend(seed_interfaces);
    let mut ancestor_queue: Vec<String> = relevant.iter().cloned().collect();
    while let Some(name) = ancestor_queue.pop() {
        let Some(bases) = inheritance_graph.get(&name) else {
            continue;
        };
        for base in bases {
            if relevant.insert(base.clone()) {
                ancestor_queue.push(base.clone());
            }
        }
    }

    let mut result = if relevant.is_empty() {
        affected
    } else {
        relevant
    };
    result.retain(|name| inheritance_graph.contains_key(name));
    result
}

fn affected_lib_extension_interface_names(
    program: &MergedProgram,
    checker_libs: &CheckerLibSet,
    affected_interfaces: &FxHashSet<String>,
) -> FxHashSet<String> {
    let user_member_names = collect_user_global_interface_member_names(program);
    let index_signature_seed_interfaces =
        collect_user_global_interfaces_with_index_signatures(program);
    let mut extension_interfaces = FxHashSet::default();
    let mut inheritance_graph: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();

    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let bases = collect_direct_base_names(lib.arena.as_ref(), interface);
            inheritance_graph
                .entry(name.clone())
                .or_default()
                .extend(bases);
            if affected_interfaces.contains(&name)
                && interface_declares_member_named(
                    lib.arena.as_ref(),
                    interface,
                    &user_member_names,
                )
            {
                extension_interfaces.insert(name);
            }
        }
    }

    let mut index_signature_affected = index_signature_seed_interfaces;
    let mut changed = true;
    while changed {
        changed = false;
        for (name, bases) in &inheritance_graph {
            if index_signature_affected.contains(name) {
                continue;
            }
            if bases
                .iter()
                .any(|base| index_signature_affected.contains(base))
            {
                changed = index_signature_affected.insert(name.clone());
            }
        }
    }
    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            if affected_interfaces.contains(&name)
                && index_signature_affected.contains(&name)
                && interface_declares_index_signature(lib.arena.as_ref(), interface)
            {
                extension_interfaces.insert(name);
            }
        }
    }

    extension_interfaces
}

fn baseline_lib_datetimeformatpart_spelling_interface_names(
    checker_libs: &CheckerLibSet,
) -> FxHashSet<String> {
    let mut interfaces = FxHashSet::default();

    for lib in &checker_libs.files {
        let Some(file_name) = Path::new(&lib.file_name)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            continue;
        };
        if !is_datetimeformatpart_spelling_baseline_lib(file_name) {
            continue;
        }
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        let text = source_file.text.as_ref();
        if !text.contains("DateTimeFormatPart") || !text.contains("DateTimeFormat") {
            continue;
        }

        if matches!(file_name, "lib.es2021.intl.d.ts" | "es2021.intl.d.ts") {
            interfaces.insert("DateTimeRangeFormatPart".to_string());
        }
        if matches!(file_name, "lib.esnext.intl.d.ts" | "esnext.intl.d.ts") {
            interfaces.insert("DateTimeFormat".to_string());
        }
    }

    interfaces
}

fn should_preserve_datetimeformatpart_spelling_baseline(checker_libs: &CheckerLibSet) -> bool {
    checker_libs.files.iter().any(|lib| {
        Path::new(&lib.file_name)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_datetimeformatpart_spelling_baseline_trigger_lib)
    })
}

fn has_esnext_umbrella_lib(checker_libs: &CheckerLibSet) -> bool {
    checker_libs.files.iter().any(|lib| {
        Path::new(&lib.file_name)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "lib.esnext.d.ts" | "esnext.d.ts"))
    })
}

fn has_parallel_order_sensitive_global_lib(checker_libs: &CheckerLibSet) -> bool {
    checker_libs.files.iter().any(|lib| {
        Path::new(&lib.file_name)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_parallel_order_sensitive_global_lib)
    })
}

fn is_parallel_order_sensitive_global_lib(file_name: &str) -> bool {
    matches!(
        file_name,
        "lib.dom.d.ts" | "dom.d.ts" | "lib.webworker.d.ts" | "webworker.d.ts"
    )
}

fn is_datetimeformatpart_spelling_baseline_trigger_lib(file_name: &str) -> bool {
    matches!(
        file_name,
        "lib.esnext.date.d.ts"
            | "esnext.date.d.ts"
            | "lib.esnext.temporal.d.ts"
            | "esnext.temporal.d.ts"
    )
}

fn is_datetimeformatpart_spelling_baseline_lib(file_name: &str) -> bool {
    matches!(
        file_name,
        "lib.es2021.intl.d.ts" | "es2021.intl.d.ts" | "lib.esnext.intl.d.ts" | "esnext.intl.d.ts"
    )
}

fn is_datetimeformatpart_spelling_baseline_diagnostic(diag: &Diagnostic) -> bool {
    if diag.code != 2552
        || diag.message_text
            != "Cannot find name 'DateTimeFormatPart'. Did you mean 'DateTimeFormat'?"
    {
        return false;
    }

    Path::new(&diag.file)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_datetimeformatpart_spelling_baseline_lib)
}

fn build_lib_bound_file_for_interface_checks(
    program: &MergedProgram,
    lib_file: &Arc<LibFile>,
    affected_interfaces: &FxHashSet<String>,
) -> tsz::parallel::BoundFile {
    let mut node_symbols = FxHashMap::default();
    if let Some(source_file) = lib_file.arena.get_source_file_at(lib_file.root_index) {
        collect_lib_interface_node_symbols(
            lib_file.arena.as_ref(),
            &source_file.statements.nodes,
            &program.globals,
            lib_file.binder.node_symbols.as_ref(),
            affected_interfaces,
            &mut node_symbols,
        );
    }

    // Deep-clone the program-wide `declaration_arenas` into the per-call map
    // so we can mutate it below. `program.declaration_arenas` is an `Arc`-shared
    // map; `Arc::clone().as_ref().clone()` gets us an owned copy of the inner
    // `DeclarationArenaMap` without disturbing the shared data.
    let mut declaration_arenas: tsz::binder::state::DeclarationArenaMap =
        (*program.declaration_arenas).clone();
    add_user_global_interface_declaration_arenas(program, &mut declaration_arenas);
    let sym_to_decl_indices = std::sync::Arc::new({
        let mut index = tsz::binder::state::SymToDeclIndicesMap::default();
        for &(sym_id, decl_idx) in declaration_arenas.keys() {
            index.entry(sym_id).or_default().push(decl_idx);
        }
        index
    });

    tsz::parallel::BoundFile {
        file_name: lib_file.file_name.clone(),
        source_file: lib_file.root_index,
        arena: Arc::clone(&lib_file.arena),
        node_symbols: std::sync::Arc::new(node_symbols),
        symbol_arenas: std::sync::Arc::clone(&program.symbol_arenas),
        declaration_arenas: std::sync::Arc::new(declaration_arenas),
        sym_to_decl_indices,
        module_declaration_exports_publicly: std::sync::Arc::new(FxHashMap::default()),
        scopes: std::sync::Arc::new(Vec::new()),
        node_scope_ids: std::sync::Arc::new(FxHashMap::default()),
        parse_diagnostics: Vec::new(),
        global_augmentations: std::sync::Arc::new(FxHashMap::default()),
        module_augmentations: std::sync::Arc::new(FxHashMap::default()),
        augmentation_target_modules: std::sync::Arc::new(FxHashMap::default()),
        flow_nodes: std::sync::Arc::new(tsz::binder::FlowNodeArena::default()),
        node_flow: std::sync::Arc::new(FxHashMap::default()),
        switch_clause_to_switch: std::sync::Arc::new(FxHashMap::default()),
        is_external_module: lib_file.binder.is_external_module,
        expando_properties: std::sync::Arc::new(FxHashMap::default()),
        file_features: tsz::binder::FileFeatures::NONE,
        lib_symbol_reverse_remap: std::sync::Arc::new(FxHashMap::default()),
        semantic_defs: std::sync::Arc::new(FxHashMap::default()),
    }
}

type LibDiagnosticFingerprint = (String, u32, u32, String);

fn lib_diagnostic_fingerprint(diag: &Diagnostic) -> LibDiagnosticFingerprint {
    (
        diag.file.clone(),
        diag.start,
        diag.code,
        diag.message_text.clone(),
    )
}

fn collect_checker_lib_baseline_fingerprints(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    checker_libs: &CheckerLibSet,
    affected_interfaces: &FxHashSet<String>,
    extension_interfaces: &FxHashSet<String>,
    program_context: &tsz::checker::context::ProgramContext,
) -> FxHashSet<LibDiagnosticFingerprint> {
    let mut fingerprints = FxHashSet::default();

    for lib_idx in 0..checker_libs.files.len() {
        let query_cache = QueryCache::new(&program.type_interner);
        let (diagnostics, _, _) = check_checker_lib_file_baseline(
            program_context,
            options,
            checker_libs,
            lib_idx,
            affected_interfaces,
            extension_interfaces,
            &query_cache,
        );
        fingerprints.extend(diagnostics.iter().map(lib_diagnostic_fingerprint));
    }

    fingerprints
}

fn collect_checker_lib_baseline_diagnostics_for_codes(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    checker_libs: &CheckerLibSet,
    affected_interfaces: &FxHashSet<String>,
    extension_interfaces: &FxHashSet<String>,
    program_context: &tsz::checker::context::ProgramContext,
    codes: &[u32],
) -> Vec<Diagnostic> {
    let code_filter = codes.iter().copied().collect::<FxHashSet<_>>();
    let mut diagnostics = Vec::new();

    for lib_idx in 0..checker_libs.files.len() {
        let query_cache = QueryCache::new(&program.type_interner);
        let (lib_diagnostics, _, _) = check_checker_lib_file_baseline(
            program_context,
            options,
            checker_libs,
            lib_idx,
            affected_interfaces,
            extension_interfaces,
            &query_cache,
        );
        diagnostics.extend(
            lib_diagnostics
                .into_iter()
                .filter(|diag| code_filter.contains(&diag.code)),
        );
    }

    diagnostics.sort_by(|a, b| {
        (a.file.as_str(), a.start, a.code, a.message_text.as_str()).cmp(&(
            b.file.as_str(),
            b.start,
            b.code,
            b.message_text.as_str(),
        ))
    });
    diagnostics.dedup_by(|a, b| lib_diagnostic_fingerprint(a) == lib_diagnostic_fingerprint(b));
    diagnostics
}

fn retain_program_induced_lib_diagnostics(
    diagnostics: &mut Vec<Diagnostic>,
    baseline_fingerprints: &FxHashSet<LibDiagnosticFingerprint>,
) {
    diagnostics.retain(|diag| !baseline_fingerprints.contains(&lib_diagnostic_fingerprint(diag)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::CliArgs;
    use std::fs;
    use std::path::PathBuf;
    use tsz_common::common::ModuleKind;

    fn collect_test_diagnostics(files: &[(&str, &str)]) -> Vec<Diagnostic> {
        let bind_results: Vec<_> = files
            .iter()
            .map(|(file_name, source)| {
                parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
            })
            .collect();
        let program = parallel::merge_bind_results(bind_results);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &program,
            &ResolvedCompilerOptions::default(),
            std::path::Path::new("/"),
            None,
            &CheckerLibSet::default(),
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics
    }

    fn collect_test_diagnostics_with_options(
        files: &[(&str, &str)],
        options: &ResolvedCompilerOptions,
        base_dir: &Path,
    ) -> Vec<Diagnostic> {
        let bind_results: Vec<_> = files
            .iter()
            .map(|(file_name, source)| {
                parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
            })
            .collect();
        let program = parallel::merge_bind_results(bind_results);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &program,
            options,
            base_dir,
            None,
            &CheckerLibSet::default(),
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics
    }

    struct FileSessionReuseOverrideGuard;

    impl Drop for FileSessionReuseOverrideGuard {
        fn drop(&mut self) {
            FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(None));
        }
    }

    fn collect_test_diagnostics_with_file_session_reuse(
        files: &[(&str, &str)],
        enabled: bool,
    ) -> Vec<Diagnostic> {
        FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(Some(enabled)));
        let _guard = FileSessionReuseOverrideGuard;
        let options = ResolvedCompilerOptions {
            no_emit: true,
            ..ResolvedCompilerOptions::default()
        };
        collect_test_diagnostics_with_options(files, &options, std::path::Path::new("/"))
    }

    fn merged_program_from_owned_files(files: Vec<(String, String)>) -> MergedProgram {
        let bind_results: Vec<_> = files
            .into_iter()
            .map(|(file_name, source)| parallel::parse_and_bind_single(file_name, source))
            .collect();
        parallel::merge_bind_results(bind_results)
    }

    #[test]
    fn detects_large_wildcard_barrel() {
        let mut files = Vec::new();
        let mut barrel = String::new();
        for i in 0..LARGE_WILDCARD_BARREL_EXPORTS {
            files.push((format!("/p/a{i}.ts"), format!("export type A{i} = {i};")));
            barrel.push_str(&format!("export * from \"./a{i}\";\n"));
        }
        files.push(("/p/index.ts".to_string(), barrel));

        let program = merged_program_from_owned_files(files);
        let work_items: Vec<usize> = (0..program.files.len()).collect();

        assert!(has_large_wildcard_barrel(&program, &work_items));
    }

    fn checker_lib_set_for_test(libs: &[(&str, &str)]) -> CheckerLibSet {
        let files = libs
            .iter()
            .map(|(file_name, source)| {
                std::sync::Arc::new(tsz::binder::lib_loader::LibFile::from_source(
                    (*file_name).to_string(),
                    (*source).to_string(),
                ))
            })
            .collect::<Vec<_>>();
        let contexts = files
            .iter()
            .map(|lib| LibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();

        CheckerLibSet {
            files,
            contexts: std::sync::Arc::new(contexts),
        }
    }

    #[test]
    fn user_only_global_interfaces_do_not_trigger_lib_recheck() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.test.d.ts",
            r#"
interface Window {
    document: object;
}
"#,
        )]);

        let program = merged_program_from_owned_files(vec![(
            "file.ts".to_string(),
            r#"
interface Result<T> {
    value?: T;
}
"#
            .to_string(),
        )]);

        let affected = affected_lib_interface_names(&program, &checker_libs);
        assert!(
            affected.is_empty(),
            "user-only global interfaces should not request default-lib recheck, got: {affected:?}"
        );
    }

    #[test]
    fn user_global_interfaces_matching_lib_names_still_trigger_lib_recheck() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.test.d.ts",
            r#"
interface Window {
    document: object;
}
"#,
        )]);

        let program = merged_program_from_owned_files(vec![(
            "file.ts".to_string(),
            r#"
interface Window {
    custom: string;
}
"#
            .to_string(),
        )]);

        let affected = affected_lib_interface_names(&program, &checker_libs);
        assert!(
            affected.contains("Window"),
            "lib-matching global interfaces must still request default-lib recheck, got: {affected:?}"
        );
    }

    #[test]
    fn parallel_order_sensitive_lib_detection_is_scoped_to_dom_like_globals() {
        let es_libs = checker_lib_set_for_test(&[("lib.es2018.d.ts", "interface Promise<T> {}\n")]);
        assert!(
            !has_parallel_order_sensitive_global_lib(&es_libs),
            "plain ES libs should stay eligible for parallel project checking"
        );

        let dom_libs =
            checker_lib_set_for_test(&[("lib.dom.d.ts", "interface Console { log(): void; }\n")]);
        assert!(
            has_parallel_order_sensitive_global_lib(&dom_libs),
            "DOM-style globals should use deterministic project checking"
        );
    }

    fn collect_test_diagnostics_with_checker_libs(
        files: &[(&str, &str)],
        checker_libs: &CheckerLibSet,
    ) -> Vec<Diagnostic> {
        let bind_results: Vec<_> = files
            .iter()
            .map(|(file_name, source)| {
                parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
            })
            .collect();
        let program = parallel::merge_bind_results(bind_results);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &program,
            &ResolvedCompilerOptions::default(),
            std::path::Path::new("/"),
            None,
            checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics
    }

    fn collect_test_diagnostics_with_lib_files(
        files: &[(&str, &str)],
        lib_files: &[std::sync::Arc<tsz::binder::lib_loader::LibFile>],
    ) -> Vec<Diagnostic> {
        collect_test_diagnostics_with_lib_files_and_options(
            files,
            lib_files,
            &ResolvedCompilerOptions::default(),
        )
    }

    fn collect_test_diagnostics_with_lib_files_and_options(
        files: &[(&str, &str)],
        lib_files: &[std::sync::Arc<tsz::binder::lib_loader::LibFile>],
        options: &ResolvedCompilerOptions,
    ) -> Vec<Diagnostic> {
        let compile_inputs = files
            .iter()
            .map(|(file_name, source)| ((*file_name).to_string(), (*source).to_string()))
            .collect::<Vec<_>>();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            lib_files,
        ));
        let checker_libs = load_checker_libs(lib_files);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &program,
            options,
            std::path::Path::new("/"),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics
    }

    fn default_cli_args_for_test() -> CliArgs {
        clap::Parser::try_parse_from(["tsz"]).expect("default args should parse")
    }

    fn resolved_options_for_es2015_strict_test() -> ResolvedCompilerOptions {
        let mut args = default_cli_args_for_test();
        args.ignore_config = true;
        args.strict = true;
        args.target = Some(crate::args::Target::Es2015);

        let mut resolved = crate::config::resolve_compiler_options(None)
            .expect("resolve default compiler options");
        crate::driver::apply_cli_overrides(&mut resolved, &args).expect("apply cli overrides");
        if matches!(resolved.printer.module, ModuleKind::None) {
            resolved.printer.module = ModuleKind::ES2015;
            resolved.checker.module = ModuleKind::ES2015;
        }
        resolved
    }

    #[test]
    fn readonly_alias_annotation_survives_consumer_first_program_check() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts"]);
        assert!(
            !lib_files.is_empty(),
            "es5.d.ts must be available for this regression"
        );
        let files = [
            (
                "/p/b.ts",
                r#"
import { Factory } from "./a.js";

Factory.cloneWith("x");
"#,
            ),
            (
                "/p/a.ts",
                r#"
import { freeze } from "./object-utils.js";

type Factory = Readonly<{
  create(name: string): string;
  cloneWith(value: string): string;
}>;

export const Factory: Factory = freeze<Factory>({
  create(name) {
    return name;
  },
  cloneWith(value) {
    return value;
  },
});
"#,
            ),
            (
                "/p/object-utils.ts",
                r#"
export function freeze<T>(value: T): Readonly<T> {
  return value;
}
"#,
            ),
        ];

        let diagnostics = collect_test_diagnostics_with_lib_files(&files, &lib_files);
        let ts2339 = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 2339)
            .collect::<Vec<_>>();

        assert!(
            ts2339.is_empty(),
            "Readonly alias annotations should not collapse to unknown in consumer-first program checks. Got: {ts2339:?}. All: {diagnostics:?}"
        );
    }

    #[test]
    fn large_project_checking_preserves_parallel_dom_globals() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts", "dom.d.ts"]);
        assert!(
            lib_files.len() >= 2,
            "es5.d.ts and dom.d.ts must be available for this regression"
        );

        let owned_files = (0..40)
            .map(|idx| {
                (
                    format!("pkg{idx}/file{idx}.ts"),
                    format!("console.log(\"file{idx}\");\nconsole.warn(\"file{idx}\");\n"),
                )
            })
            .collect::<Vec<_>>();
        let files = owned_files
            .iter()
            .map(|(file_name, source)| (file_name.as_str(), source.as_str()))
            .collect::<Vec<_>>();
        let options = ResolvedCompilerOptions {
            no_emit: true,
            ..ResolvedCompilerOptions::default()
        };

        let reused_diagnostics = {
            FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(Some(true)));
            let _guard = FileSessionReuseOverrideGuard;
            collect_test_diagnostics_with_lib_files_and_options(&files, &lib_files, &options)
        };
        let disabled_diagnostics = {
            FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(Some(false)));
            let _guard = FileSessionReuseOverrideGuard;
            collect_test_diagnostics_with_lib_files_and_options(&files, &lib_files, &options)
        };
        let console_member_errors = reused_diagnostics
            .iter()
            .chain(disabled_diagnostics.iter())
            .filter(|diagnostic| diagnostic.code == 2339)
            .collect::<Vec<_>>();

        assert!(
            console_member_errors.is_empty(),
            "large-project DOM globals must not be order-dependent. TS2339: {console_member_errors:?}. Reused: {reused_diagnostics:?}. Disabled: {disabled_diagnostics:?}"
        );
    }

    #[test]
    fn file_session_reuse_preserves_multifile_diagnostics() {
        let files = [
            (
                "a.ts",
                "interface Alpha { kind: \"alpha\"; count: number }\nconst a: Alpha = { kind: \"alpha\", count: \"nope\" };\n",
            ),
            (
                "b.ts",
                "interface Beta { kind: \"beta\"; count: number }\nconst b: Beta = { kind: \"beta\", count: \"nope\" };\n",
            ),
            (
                "c.ts",
                "interface Gamma { kind: \"gamma\"; count: number }\nconst c: Gamma = { kind: \"gamma\", count: \"nope\" };\n",
            ),
        ];

        let default_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, false);
        let reused_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, true);

        assert_eq!(
            reused_diagnostics, default_diagnostics,
            "file-session reuse must preserve byte-identical diagnostics"
        );
        assert!(
            !default_diagnostics.is_empty(),
            "fixture should exercise real checker diagnostics"
        );
    }

    #[test]
    fn file_session_reuse_preserves_parallel_multifile_diagnostics() {
        let owned_files = (0..40)
            .map(|idx| {
                (
                    format!("pkg{idx}/file{idx}.ts"),
                    format!("export {{}};\nconst value{idx}: number = \"nope\";\n"),
                )
            })
            .collect::<Vec<_>>();
        let files = owned_files
            .iter()
            .map(|(file_name, source)| (file_name.as_str(), source.as_str()))
            .collect::<Vec<_>>();

        let default_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, false);
        let reused_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, true);

        assert_eq!(
            reused_diagnostics, default_diagnostics,
            "parallel file-session reuse must preserve byte-identical diagnostics"
        );
        assert_eq!(
            default_diagnostics.len(),
            owned_files.len(),
            "fixture should produce one checker diagnostic per file"
        );
    }

    #[test]
    fn no_check_collect_diagnostics_keeps_parse_errors_and_skips_type_errors() {
        let options = ResolvedCompilerOptions {
            no_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "const value: string = 1;\nconst broken = ;\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&1109),
            "expected --noCheck diagnostics to keep TS1109 parse error, got: {diagnostics:?}"
        );
        assert!(
            !codes.contains(&2322),
            "expected --noCheck diagnostics to skip TS2322 type error, got: {diagnostics:?}"
        );
    }

    #[test]
    fn no_check_path_emits_isolated_declarations_ts9007() {
        // Issue #3709: `--noCheck --isolatedDeclarations` previously dropped
        // TS9007/TS9011/etc. tsc still reports these because they gate
        // declaration emission, not type checking.
        let mut options = ResolvedCompilerOptions {
            no_check: true,
            ..ResolvedCompilerOptions::default()
        };
        options.checker.isolated_declarations = true;

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "export function f(x) { return x; }\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&9007),
            "expected --noCheck --isolatedDeclarations to surface TS9007, got: {diagnostics:?}"
        );
    }

    #[test]
    fn no_check_without_isolated_declarations_does_not_run_isolated_decl_pass() {
        // Without --isolatedDeclarations, the isolated-decl pass must not
        // fire and produce TS9007.
        let options = ResolvedCompilerOptions {
            no_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "export function f(x) { return x; }\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            !codes.contains(&9007),
            "TS9007 must not fire under --noCheck without --isolatedDeclarations, got: {diagnostics:?}"
        );
    }

    #[test]
    fn no_check_with_declaration_emit_still_suppresses_type_errors() {
        // Issue #3733: under `--noCheck --declaration`, the regular checker
        // pipeline must run so declaration emit can pick up inferred types
        // (return types, contextual property types). But type-error
        // diagnostics (TS2322 etc.) must still be suppressed — `--noCheck`
        // means "don't surface type checking errors".
        let options = ResolvedCompilerOptions {
            no_check: true,
            emit_declarations: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "export const x: string = 1;\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            !codes.contains(&2322),
            "TS2322 must not fire under --noCheck --declaration, got: {diagnostics:?}"
        );
    }

    #[test]
    fn skip_lib_check_pure_declaration_no_emit_skips_semantic_diagnostics() {
        let options = ResolvedCompilerOptions {
            no_emit: true,
            skip_lib_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "index.d.ts",
                r#"
export type UsesMissing = Missing;
export interface Broken {
    value: ;
}
"#,
            )],
            &options,
            std::path::Path::new("/"),
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code < 2000),
            "parse diagnostics must still surface under skipLibCheck: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2304),
            "skipLibCheck must suppress declaration-file semantic diagnostics: {diagnostics:?}"
        );
    }

    #[test]
    fn skip_lib_check_mixed_project_still_checks_source_files() {
        let options = ResolvedCompilerOptions {
            no_emit: true,
            skip_lib_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                ("types.d.ts", "export type UsesMissing = Missing;\n"),
                ("main.ts", "const value: string = 1;\n"),
            ],
            &options,
            std::path::Path::new("/"),
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2322),
            "non-declaration source files must still be checked under skipLibCheck: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2304),
            "declaration-file semantic diagnostics must remain suppressed: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_preserves_builtin_lib_ts2552_spelling_baseline() {
        let checker_libs = checker_lib_set_for_test(&[
            (
                "lib.esnext.intl.d.ts",
                r#"
declare namespace Intl {
    interface DateTimeFormat {
        formatToParts(): DateTimeFormatPart[];
    }
}
"#,
            ),
            (
                "lib.esnext.temporal.d.ts",
                r#"
declare namespace Temporal {
    interface Instant {}
}
"#,
            ),
        ]);

        let diagnostics = collect_test_diagnostics_with_checker_libs(
            &[("test.ts", "const value = new Intl.DateTimeFormat();\n")],
            &checker_libs,
        );
        let ts2552 = diagnostics
            .iter()
            .filter(|diag| diag.code == 2552)
            .collect::<Vec<_>>();

        assert_eq!(
            ts2552.len(),
            1,
            "expected one baseline lib TS2552 diagnostic, got: {diagnostics:?}"
        );
        assert!(
            ts2552[0]
                .message_text
                .contains("Cannot find name 'DateTimeFormatPart'. Did you mean 'DateTimeFormat'?"),
            "expected DateTimeFormatPart spelling suggestion, got: {ts2552:?}"
        );
        assert_eq!(ts2552[0].file, "lib.esnext.intl.d.ts");
    }

    #[test]
    fn collect_diagnostics_skips_builtin_lib_ts2552_without_temporal_trigger_lib() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.esnext.intl.d.ts",
            r#"
declare namespace Intl {
    interface DateTimeFormat {
        formatToParts(): DateTimeFormatPart[];
    }
}
"#,
        )]);

        let diagnostics = collect_test_diagnostics_with_checker_libs(
            &[("test.ts", "const value = new Intl.DateTimeFormat();\n")],
            &checker_libs,
        );

        assert!(
            diagnostics.iter().all(|diag| diag.code != 2552),
            "expected DateTimeFormatPart baseline to require Temporal/Date libs, got: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_ignores_unrelated_builtin_lib_ts2552_spelling_baseline() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.esnext.intl.d.ts",
            r#"
declare namespace Intl {
    interface DateTimeFormatPart {}
    interface DateTimeFormat {
        formatToParts(): DateTimeFormatParts[];
    }
}
"#,
        )]);

        let diagnostics = collect_test_diagnostics_with_checker_libs(
            &[("test.ts", "const value = new Intl.DateTimeFormat();\n")],
            &checker_libs,
        );

        assert!(
            diagnostics.iter().all(|diag| diag.code != 2552),
            "expected unrelated baseline lib TS2552 diagnostics to stay filtered, got: {diagnostics:?}"
        );
    }

    fn collect_es2015_default_lib_diagnostics(source: &str) -> Vec<Diagnostic> {
        collect_es2015_default_lib_diagnostics_with_options(source, |_: &mut _| {})
    }

    fn collect_es2015_default_lib_diagnostics_with_options(
        source: &str,
        configure: impl FnOnce(&mut ResolvedCompilerOptions),
    ) -> Vec<Diagnostic> {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(&file_path, source).expect("write source");

        let mut resolved = resolved_options_for_es2015_strict_test();
        configure(&mut resolved);
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics
    }

    #[test]
    fn cloned_checker_libs_preserve_strict_builtin_iterator_return() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
declare const map: Map<string, number>;
const value: number = map.values().next().value;
interface Next<A> {
    readonly done?: boolean;
    readonly value: A;
}
const result: Next<number> = map.values().next();
"#,
        );
        let ts2322_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        assert_eq!(
            ts2322_count, 2,
            "expected cloned checker libs to preserve strict built-in iterator return diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn es2015_local_interface_t_shadows_lib_heritage_type_parameters() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
interface T { f(x: number): void }
declare var t: T;
t.f("s");
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2345),
            "expected TS2345 for T.f argument type, got: {diagnostics:?}"
        );
        assert!(
            diagnostics.iter().all(|diag| diag.code != 2339),
            "did not expect TS2339 from a stale local T shape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn es2015_destructuring_reduce_concat_reports_overload_and_iterability() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
declare var tuple: [boolean, number, ...string[]];

const [a, b, c, ...rest] = tuple;

declare var receiver: typeof tuple;

[...receiver] = tuple;

const [oops1] = [1, 2, 3].reduce((accu, el) => accu.concat(el), []);
"#,
        );
        let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();

        assert!(
            codes.contains(&2488),
            "expected TS2488 for destructuring the failed reduce result, got: {diagnostics:?}"
        );
        assert!(
            codes.contains(&2769),
            "expected TS2769 for the nested reduce/concat overload failure, got: {diagnostics:?}"
        );
    }

    fn mapped_type_indexed_access_constraint_repro() -> &'static str {
        r#"type Identity<T> = { [K in keyof T]: T[K] };

type M0 = { a: 1, b: 2 };

type M1 = { [K in keyof Partial<M0>]: M0[K] };

type M2 = { [K in keyof Required<M1>]: M1[K] };

type M3 = { [K in keyof Identity<Partial<M0>>]: M0[K] };

function foo<K extends keyof M0>(m1: M1[K], m2: M2[K], m3: M3[K]) {
    m1.toString();
    m1?.toString();
    m2.toString();
    m2?.toString();
    m3.toString();
    m3?.toString();
}

type Obj = {
    a: 1,
    b: 2
};

const mapped: { [K in keyof Partial<Obj>]: Obj[K] } = {};

const resolveMapped = <K extends keyof typeof mapped>(key: K) => mapped[key].toString();

const arr = ["foo", "12", 42] as const;

type Mappings = { foo: boolean, "12": number, 42: string };

type MapperArgs<K extends (typeof arr)[number]> = {
    v: K,
    i: number
};

type SetOptional<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

type PartMappings = SetOptional<Mappings, "foo">;

const mapper: { [K in keyof PartMappings]: (o: MapperArgs<K>) => PartMappings[K] } = {
    foo: ({ v, i }) => v.length + i > 4,
    "12": ({ v, i }) => Number(v) + i,
    42: ({ v, i }) => `${v}${i}`,
};

const resolveMapper1 = <K extends keyof typeof mapper>(
    key: K, o: MapperArgs<K>) => mapper[key](o);

const resolveMapper2 = <K extends keyof typeof mapper>(
    key: K, o: MapperArgs<K>) => mapper[key]?.(o);
"#
    }

    #[test]
    fn jsx_attribute_comma_expression_survives_into_bind_results() {
        let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        [s: string]: any;
    }
}

const class1 = "foo";
const class2 = "bar";
const elem = <div className={class1, class2}/>;
"#;

        let result = parallel::parse_and_bind_single("file.tsx".to_string(), source.to_string());
        let codes: Vec<u32> = result.parse_diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&18007),
            "expected TS18007 in bind-result parse diagnostics, got: {codes:?}"
        );
    }

    #[test]
    fn jsx_attribute_comma_expression_reports_ts18007_in_cli_diagnostics() {
        let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        [s: string]: any;
    }
}

const class1 = "foo";
const class2 = "bar";
const elem = <div className={class1, class2}/>;
"#;

        let diagnostics = collect_test_diagnostics(&[("file.tsx", source)]);
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&18007),
            "expected CLI diagnostics to include TS18007, got: {diagnostics:?}"
        );
        assert!(
            codes.contains(&2695),
            "expected CLI diagnostics to include TS2695, got: {diagnostics:?}"
        );
    }

    #[test]
    fn jsx_invalid_namespace_start_keeps_colon_ts1109_in_bind_results() {
        let source = "declare var React: any;\nvar x = <:a attr={\"value\"} />;\n";
        let result = parallel::parse_and_bind_single("file.tsx".to_string(), source.to_string());
        let less_than_pos = source.find('<').expect("opening angle") as u32;
        let colon_pos = source[less_than_pos as usize + 1..]
            .find(':')
            .map(|offset| less_than_pos + 1 + offset as u32)
            .expect("colon");
        let expr_expected_positions: Vec<u32> = result
            .parse_diagnostics
            .iter()
            .filter(|diag| diag.code == 1109)
            .map(|diag| diag.start)
            .collect();

        assert!(
            expr_expected_positions.contains(&less_than_pos),
            "expected TS1109 at '<', got: {expr_expected_positions:?}"
        );
        assert!(
            expr_expected_positions.contains(&colon_pos),
            "expected TS1109 at ':', got: {expr_expected_positions:?}"
        );
    }

    #[test]
    fn jsx_invalid_namespace_start_keeps_colon_ts1109_in_cli_diagnostics() {
        let source = "declare var React: any;\nvar x = <:a attr={\"value\"} />;\n";
        let diagnostics = collect_test_diagnostics(&[("file.tsx", source)]);
        let less_than_pos = source.find('<').expect("opening angle") as u32;
        let colon_pos = source[less_than_pos as usize + 1..]
            .find(':')
            .map(|offset| less_than_pos + 1 + offset as u32)
            .expect("colon");
        let expr_expected_positions: Vec<u32> = diagnostics
            .iter()
            .filter(|diag| diag.code == 1109)
            .map(|diag| diag.start)
            .collect();

        assert!(
            expr_expected_positions.contains(&less_than_pos),
            "expected CLI TS1109 at '<', got: {diagnostics:?}"
        );
        assert!(
            expr_expected_positions.contains(&colon_pos),
            "expected CLI TS1109 at ':', got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_mapped_type_nullish_indexed_reads() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(&file_path, mapped_type_indexed_access_constraint_repro())
            .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));

        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());
        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;
        let ts18048_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::IS_POSSIBLY_UNDEFINED)
            .count();
        let ts2532_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED)
            .count();
        let ts2722_count = diagnostics
            .iter()
            .filter(|diag| {
                diag.code == diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED
            })
            .count();
        let ts2349_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE)
            .count();

        assert_eq!(
            ts18048_count, 3,
            "Expected collect_diagnostics to preserve three TS18048 diagnostics, got: {diagnostics:?}"
        );
        assert_eq!(
            ts2532_count, 1,
            "Expected one TS2532 for mapped[key].toString(), got: {diagnostics:?}"
        );
        assert_eq!(
            ts2722_count, 1,
            "Expected one TS2722 for mapper[key](o), got: {diagnostics:?}"
        );
        assert_eq!(
            ts2349_count, 0,
            "Did not expect TS2349 for mapper[key](o), got: {diagnostics:?}"
        );
    }

    fn resolved_options_for_esnext_strict_test() -> ResolvedCompilerOptions {
        let mut args = default_cli_args_for_test();
        args.ignore_config = true;
        args.strict = true;
        args.target = Some(crate::args::Target::EsNext);

        let mut resolved = crate::config::resolve_compiler_options(None)
            .expect("resolve default compiler options");
        crate::driver::apply_cli_overrides(&mut resolved, &args).expect("apply cli overrides");
        if matches!(resolved.printer.module, ModuleKind::None) {
            resolved.printer.module = ModuleKind::ESNext;
            resolved.checker.module = ModuleKind::ESNext;
        }
        resolved
    }

    #[test]
    fn test_compile_inner_program_build_promise_is_assignable_to_promise_like() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
declare const p: Promise<number>;
const q: PromiseLike<number> = p;
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        assert!(
            diagnostics.is_empty(),
            "Expected compile-inner program build Promise<T> -> PromiseLike<T> assignability, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_compile_inner_program_reports_ts2851_for_async_iterator_await_using() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
declare const ai: AsyncIterator<string, undefined>;
declare const aio: AsyncIteratorObject<string, undefined, unknown>;
declare const ag: AsyncGenerator<string, void>;

async function f() {
    await using it0 = aio;
    await using it1 = ag;
    await using it2 = ai;
}
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_esnext_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        let ts2851_count = diagnostics.iter().filter(|diag| diag.code == 2851).count();
        assert_eq!(
            ts2851_count, 1,
            "Expected one TS2851 for await using AsyncIterator, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_compile_inner_program_reports_ts2456_for_recursive_mapped_type_aliases() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
type Recurse = {
    [K in keyof Recurse]: Recurse[K]
}

type Recurse1 = {
    [K in keyof Recurse2]: Recurse2[K]
}

type Recurse2 = {
    [K in keyof Recurse1]: Recurse1[K]
}
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        let ts2456_count = diagnostics.iter().filter(|diag| diag.code == 2456).count();
        assert_eq!(
            ts2456_count, 3,
            "Expected TS2456 for the recursive mapped aliases, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_invariant_generic_error_elaboration_ts2322() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"// Repro from #19746

const wat: Runtype<any> = Num;
const Foo = Obj({ foo: Num })

interface Runtype<A> {
  constraint: Constraint<this>
  witness: A
}

interface Num extends Runtype<number> {
  tag: 'number'
}
declare const Num: Num

interface Obj<O extends { [_ in string]: Runtype<any> }> extends Runtype<{[K in keyof O]: O[K]['witness'] }> {}
declare function Obj<O extends { [_: string]: Runtype<any> }>(fields: O): Obj<O>;

interface Constraint<A extends Runtype<any>> extends Runtype<A['witness']> {
  underlying: A,
  check: (x: A['witness']) => void,
}
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let direct_lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        let direct_checker_libs = CheckerLibSet {
            files: lib_files.clone(),
            contexts: Arc::new(direct_lib_contexts.clone()),
        };
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let parallel_result =
            parallel::check_files_parallel(&program, &resolved.checker, &lib_files);
        let _parallel_ts2322_count = parallel_result
            .file_results
            .iter()
            .flat_map(|file| file.diagnostics.iter())
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        let rebuilt_binder = create_binder_from_bound_file(&program.files[0], &program, 0);
        program
            .type_interner
            .set_no_unchecked_indexed_access(resolved.checker.no_unchecked_indexed_access);
        program
            .type_interner
            .set_exact_optional_property_types(resolved.checker.exact_optional_property_types);
        let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
        let mut checker = CheckerState::with_options(
            &program.files[0].arena,
            &rebuilt_binder,
            &query_cache,
            program.files[0].file_name.clone(),
            &resolved.checker,
        );
        checker.ctx.set_lib_contexts(direct_lib_contexts.clone());
        checker
            .ctx
            .set_actual_lib_file_count(direct_lib_contexts.len());
        let all_arenas = Arc::new(
            program
                .files
                .iter()
                .map(|file| Arc::clone(&file.arena))
                .collect::<Vec<_>>(),
        );
        let all_binders = Arc::new(vec![Arc::new(create_binder_from_bound_file(
            &program.files[0],
            &program,
            0,
        ))]);
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        if let Some(ref skel) = program.skeleton_index {
            let (exact, patterns) = skel.build_declared_module_sets();
            checker.ctx.set_declared_modules_from_skeleton(Arc::new(
                tsz::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns),
            ));
            checker
                .ctx
                .set_expando_index_from_skeleton(Arc::clone(&skel.expando_properties));
        }
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(0);
        checker.check_source_file(program.files[0].source_file);

        let source_file = program.files[0]
            .arena
            .get(program.files[0].source_file)
            .and_then(|node| program.files[0].arena.get_source_file(node))
            .expect("missing source file");
        let var_stmt_idx = *source_file
            .statements
            .nodes
            .first()
            .expect("variable statement");
        let var_stmt_node = program.files[0]
            .arena
            .get(var_stmt_idx)
            .expect("var stmt node");
        let var_stmt_data = program.files[0]
            .arena
            .get_variable(var_stmt_node)
            .expect("var stmt data");
        let decl_list_idx = *var_stmt_data
            .declarations
            .nodes
            .first()
            .expect("declaration list");
        let decl_list_node = program.files[0]
            .arena
            .get(decl_list_idx)
            .expect("decl list node");
        let decl_list_data = program.files[0]
            .arena
            .get_variable(decl_list_node)
            .expect("decl list data");
        let decl_idx = *decl_list_data
            .declarations
            .nodes
            .first()
            .expect("declaration");
        let decl_node = program.files[0].arena.get(decl_idx).expect("decl node");
        let decl = program.files[0]
            .arena
            .get_variable_declaration(decl_node)
            .expect("decl data");
        let _source_type = checker.get_type_of_node(decl.initializer);
        let target_type = checker.get_type_from_type_node(decl.type_annotation);
        let _read_constraint_type =
            |object_type| match tsz_solver::QueryDatabase::resolve_property_access(
                &query_cache,
                object_type,
                "constraint",
            ) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id, ..
                } => Some(type_id),
                _ => None,
            };
        let _evaluated_target_type = {
            let mut evaluator =
                tsz_solver::TypeEvaluator::with_resolver(&program.type_interner, &checker.ctx);
            evaluator.evaluate(target_type)
        };
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());
        let direct_diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &direct_checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;
        let direct_ts2322_count = direct_diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        assert_eq!(
            direct_ts2322_count, 2,
            "Expected collect_diagnostics with direct lib contexts to preserve two TS2322 diagnostics, got: {direct_diagnostics:?}"
        );

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        let ts2322_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        assert_eq!(
            ts2322_count, 2,
            "Expected compile-inner collect_diagnostics to preserve two TS2322 diagnostics, got: {diagnostics:?}"
        );
    }

    /// TS2883: Nested `node_modules` types should be detected as non-portable.
    /// When an exported variable's inferred type references a type from a
    /// nested `node_modules` (e.g., `foo/node_modules/nested`), tsz must
    /// emit TS2883 even when the nested package lacks a `package.json`.
    #[test]
    fn test_ts2883_nested_node_modules_non_portable_type() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        // Create nested node_modules structure:
        // r/node_modules/foo/node_modules/nested/index.d.ts
        let nested_dir = dir.path().join("r/node_modules/foo/node_modules/nested");
        std::fs::create_dir_all(&nested_dir).expect("create nested dir");
        std::fs::write(
            nested_dir.join("index.d.ts"),
            "export interface NestedProps {}\n",
        )
        .expect("write nested/index.d.ts");

        let foo_dir = dir.path().join("r/node_modules/foo");
        std::fs::write(
            foo_dir.join("index.d.ts"),
            r#"import { NestedProps } from "nested";
export interface SomeProps {}
export function foo(): [SomeProps, NestedProps];
"#,
        )
        .expect("write foo/index.d.ts");

        std::fs::write(
            dir.path().join("r/entry.ts"),
            r#"import { foo } from "foo";
export const x = foo();
"#,
        )
        .expect("write r/entry.ts");

        let mut resolved = resolved_options_for_es2015_strict_test();
        resolved.checker.module = ModuleKind::CommonJS;
        resolved.printer.module = ModuleKind::CommonJS;
        resolved.checker.emit_declarations = true;

        let file_paths = vec![dir.path().join("r/entry.ts")];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());
        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        let ts2883_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == 2883)
            .collect();
        assert!(
            !ts2883_diags.is_empty(),
            "Expected at least one TS2883 diagnostic for nested node_modules type reference, got: {diagnostics:?}"
        );
        assert!(
            ts2883_diags[0].message_text.contains("NestedProps"),
            "TS2883 message should reference 'NestedProps', got: {}",
            ts2883_diags[0].message_text
        );
        assert!(
            ts2883_diags[0]
                .message_text
                .contains("foo/node_modules/nested"),
            "TS2883 message should reference 'foo/node_modules/nested', got: {}",
            ts2883_diags[0].message_text
        );
    }

    #[test]
    fn test_collect_diagnostics_keeps_unimported_external_module_type_alias_unresolved() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        std::fs::write(
            dir.path().join("Helpers.ts"),
            r#"
export type StringKeyOf<TObj> = Extract<string, keyof TObj>;
"#,
        )
        .expect("write Helpers.ts");
        std::fs::write(
            dir.path().join("FromFactor.ts"),
            r#"
export type RowToColumns<TColumns> = {
    [TName in StringKeyOf<TColumns>]: any;
};
"#,
        )
        .expect("write FromFactor.ts");

        let mut resolved = resolved_options_for_es2015_strict_test();
        resolved.checker.module = ModuleKind::CommonJS;
        resolved.printer.module = ModuleKind::CommonJS;
        resolved.checker.emit_declarations = true;

        let file_paths = vec![
            dir.path().join("Helpers.ts"),
            dir.path().join("FromFactor.ts"),
        ];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());
        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == diagnostic_codes::CANNOT_FIND_NAME
                    && diag.message_text.contains("StringKeyOf")
            }),
            "Expected TS2304 for unimported external-module type alias in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_is_declaration_file() {
        assert!(is_declaration_file("types.d.ts"));
        assert!(is_declaration_file("index.d.mts"));
        assert!(is_declaration_file("index.d.cts"));
        assert!(is_declaration_file("native.d.node.ts"));
        assert!(is_declaration_file("/path/to/file.d.ts"));
        assert!(is_declaration_file("/path/to/file.d.mts"));
        assert!(is_declaration_file("/path/to/file.d.cts"));
        assert!(is_declaration_file("/path/to/file.d.node.ts"));

        assert!(!is_declaration_file("index.ts"));
        assert!(!is_declaration_file("index.mts"));
        assert!(!is_declaration_file("index.cts"));
        assert!(!is_declaration_file("index.js"));
    }

    #[test]
    fn test_transitive_module_export_bridge_infers_type_only_flags() {
        let a_file = parallel::parse_and_bind_single(
            "/a.ts".to_string(),
            "export class A {}\nexport class B {}".to_string(),
        );
        let b_file = parallel::parse_and_bind_single(
            "/b.ts".to_string(),
            "export type * from \"./a\";".to_string(),
        );
        let c_file = parallel::parse_and_bind_single(
            "/c.ts".to_string(),
            "export * from \"./b\";".to_string(),
        );
        let d_file = parallel::parse_and_bind_single(
            "/d.ts".to_string(),
            r#"import { A, B } from "./c";
let _: A = new A();
let __: B = new B();"#
                .to_string(),
        );

        let program = parallel::merge_bind_results(vec![a_file, b_file, c_file, d_file]);
        let d_idx = 3;
        let d_bound = &program.files[d_idx];
        let mut binder = create_binder_from_bound_file(d_bound, &program, d_idx);

        let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
        resolved_module_paths.insert((d_idx, "./c".to_string()), 2);
        resolved_module_paths.insert((2, "./b".to_string()), 1);
        resolved_module_paths.insert((1, "./a".to_string()), 0);

        let module_specifiers = collect_module_specifiers(&d_bound.arena, d_bound.source_file);
        for (specifier, _, _, _) in &module_specifiers {
            if let Some(&target_idx) = resolved_module_paths.get(&(d_idx, specifier.clone())) {
                propagate_module_export_maps(
                    &mut binder,
                    specifier,
                    target_idx,
                    &program,
                    &resolved_module_paths,
                );
            }
        }

        assert!(binder.wildcard_reexports.contains_key("./c"));
        let c_wildcards = binder
            .wildcard_reexports
            .get("./c")
            .expect("expected wildcard re-exports for ./c");
        assert_eq!(c_wildcards, &vec!["./b".to_string()]);

        let b_wildcards = binder
            .wildcard_reexports
            .get("./b")
            .expect("expected wildcard re-exports for ./b");
        assert_eq!(b_wildcards, &vec!["./a".to_string()]);

        let b_type_only = binder
            .wildcard_reexports_type_only
            .get("./b")
            .expect("expected type-only metadata for ./b");
        assert!(
            b_type_only
                .iter()
                .any(|(source, is_type_only)| source == "./a" && *is_type_only)
        );

        let exports_via_c = binder
            .resolve_import_with_reexports_type_only("./c", "A")
            .expect("expected A to resolve via wildcard chain");
        assert!(exports_via_c.1, "A should be considered type-only via ./b");
    }

    #[test]
    fn test_collect_diagnostics_suppresses_ts2307_for_local_ambient_module() {
        let diagnostics = collect_test_diagnostics(&[
            (
                "/project/demo.d.ts",
                r#"
declare namespace demoNS {
    function f(): void;
}
declare module "demoModule" {
    import alias = demoNS;
    export = alias;
}
"#,
            ),
            (
                "/project/user.ts",
                r#"
import { f } from "demoModule";
let x1: string = demoNS.f;
let x2: string = f;
"#,
            ),
        ]);

        let codes = diagnostics.iter().map(|d| d.code).collect::<Vec<_>>();
        assert!(
            !codes.contains(&2307),
            "Did not expect TS2307 when a local ambient module declaration matches the import. Diagnostics: {diagnostics:?}"
        );
        assert_eq!(
            codes.iter().filter(|&&code| code == 2322).count(),
            2,
            "Expected the import to still resolve and produce two downstream TS2322 diagnostics. Diagnostics: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_source_local_duplicate_package_paths() {
        let dir = std::env::temp_dir().join("tsz_check_duplicate_package_global_merge");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("tests")).unwrap();
        fs::create_dir_all(dir.join("node_modules/@types/react")).unwrap();
        fs::create_dir_all(dir.join("tests/node_modules/@types/react")).unwrap();

        fs::write(
            dir.join("node_modules/@types/react/package.json"),
            r#"{"name":"@types/react","version":"16.4.6"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("tests/node_modules/@types/react/package.json"),
            r#"{"name":"@types/react","version":"16.4.6"}"#,
        )
        .unwrap();

        let root_react_path = dir.join("node_modules/@types/react/index.d.ts");
        let tests_react_path = dir.join("tests/node_modules/@types/react/index.d.ts");
        // Both stubs must be proper external modules so that import resolution
        // sees them as valid modules rather than producing TS2306/TS2669.
        fs::write(
            &root_react_path,
            "export declare function createElement(tag: string): any;\n",
        )
        .unwrap();
        fs::write(
            &tests_react_path,
            "export declare function createElement(tag: string): any;\n",
        )
        .unwrap();

        let src_index = dir.join("src/index.ts");
        let tests_index = dir.join("tests/index.ts");

        let options = ResolvedCompilerOptions {
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    src_index.to_str().unwrap(),
                    "import * as React from 'react';\nexport var x = 1;\n",
                ),
                (
                    tests_index.to_str().unwrap(),
                    "import * as React from 'react';\nexport var y = 2;\n",
                ),
                (
                    root_react_path.to_str().unwrap(),
                    "export declare function createElement(tag: string): any;\n",
                ),
                (
                    tests_react_path.to_str().unwrap(),
                    "export declare function createElement(tag: string): any;\n",
                ),
            ],
            &options,
            &dir,
        );

        // With valid module stubs, both imports should resolve successfully.
        // The test primarily validates that having duplicate @types/react at
        // different node_modules depths does not crash or produce spurious errors.
        // No TS2307/TS2306/TS2669 diagnostics should appear.
        let error_codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert!(
            !error_codes.contains(&2307)
                && !error_codes.contains(&2306)
                && !error_codes.contains(&2669),
            "expected no module resolution errors with valid react stubs, got: {diagnostics:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_suppresses_ts2339_after_type_only_export_equals_namespace_use() {
        let dir = std::env::temp_dir().join("tsz_check_type_only_export_equals_namespace_use");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let a_path = dir.join("a.ts");
        let b_path = dir.join("b.ts");
        let f_path = dir.join("f.ts");

        let a_source = "export class A {}\n";
        let b_source = "import type * as types from './a';\nexport = types;\n";
        let f_source = "import * as types from './b';\nnew types.A();\n";

        fs::write(&a_path, a_source).unwrap();
        fs::write(&b_path, b_source).unwrap();
        fs::write(&f_path, f_source).unwrap();

        let options = ResolvedCompilerOptions {
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            es_module_interop: true,
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (a_path.to_str().unwrap(), a_source),
                (b_path.to_str().unwrap(), b_source),
                (f_path.to_str().unwrap(), f_source),
            ],
            &options,
            &dir,
        );

        let f_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| Path::new(&diag.file) == f_path.as_path() && diag.code != 2318)
            .collect();

        assert!(
            f_diags.iter().any(|diag| diag.code == 1361),
            "expected TS1361 on namespace use of a type-only export= chain, got: {f_diags:?}"
        );
        assert!(
            f_diags.iter().all(|diag| diag.code != 2339),
            "did not expect follow-on TS2339 once TS1361 fired, got: {f_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_allows_checked_js_module_exports_type_only_require() {
        let dir = std::env::temp_dir().join("tsz_check_js_module_exports_type_only_require");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let importer_path = dir.join("importer.cjs");
        let exporter_path = dir.join("exporter.mts");

        let importer_source = "const Foo = require(\"./exporter.mjs\");\nnew Foo();\n";
        let exporter_source =
            "export default class Foo {}\nexport type { Foo as \"module.exports\" };\n";

        fs::write(&importer_path, importer_source).unwrap();
        fs::write(&exporter_path, exporter_source).unwrap();

        let options = ResolvedCompilerOptions {
            allow_js: true,
            check_js: true,
            module_resolution: Some(crate::config::ModuleResolutionKind::Node16),
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::Node18,
                target: tsz_common::common::ScriptTarget::ES2023,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::Node18,
                target: tsz_common::common::ScriptTarget::ES2023,
                ..Default::default()
            },
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (importer_path.to_str().unwrap(), importer_source),
                (exporter_path.to_str().unwrap(), exporter_source),
            ],
            &options,
            &dir,
        );

        let importer_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| Path::new(&diag.file) == importer_path.as_path())
            .collect();

        assert!(
            importer_diags.is_empty(),
            "expected checked CommonJS require() of a type-only \
             \"module.exports\" binding to avoid diagnostics, got: {importer_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_rejects_exports_in_cjs_file_with_esm_syntax() {
        let dir = std::env::temp_dir().join("tsz_check_js_cjs_exports_with_esm_syntax");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let two_path = dir.join("2.cjs");
        let three_path = dir.join("3.cjs");
        let four_path = dir.join("4.cjs");
        let five_path = dir.join("5.cjs");

        let two_source = "exports.foo = 0;\n";
        let three_source = "import \"foo\";\nexports.foo = {};\n";
        let four_source = ";\n";
        let five_source =
            "import two from \"./2.cjs\";\nimport three from \"./3.cjs\";\ntwo.foo;\nthree.foo;\n";

        let options = ResolvedCompilerOptions {
            allow_js: true,
            check_js: true,
            module_resolution: Some(crate::config::ModuleResolutionKind::NodeNext),
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::Node20,
                target: tsz_common::common::ScriptTarget::ES2022,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::Node20,
                target: tsz_common::common::ScriptTarget::ES2022,
                no_types_and_symbols: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (two_path.to_str().unwrap(), two_source),
                (three_path.to_str().unwrap(), three_source),
                (four_path.to_str().unwrap(), four_source),
                (five_path.to_str().unwrap(), five_source),
            ],
            &options,
            &dir,
        );

        let three_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| Path::new(&diag.file) == three_path.as_path())
            .collect();
        let five_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| Path::new(&diag.file) == five_path.as_path())
            .collect();

        assert!(
            three_diags.iter().any(|diag| {
                diag.code == 2304 && diag.message_text.contains("Cannot find name 'exports'")
            }),
            "expected TS2304 for exports in a .cjs file with ESM syntax, got: {three_diags:?}"
        );
        assert!(
            five_diags
                .iter()
                .any(|diag| { diag.code == 1192 && diag.message_text.contains("Module '\"3\"'") }),
            "expected TS1192 for default import from the ESM-syntax .cjs file, got file diagnostics: {five_diags:?}; all diagnostics: {diagnostics:?}"
        );
        assert!(
            five_diags.iter().all(|diag| {
                !(diag.code == 1192 && diag.message_text.contains("Module '\"2\"'"))
            }),
            "did not expect TS1192 for default import from plain CommonJS .cjs, got: {five_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_bundler_dual_package_modes_do_not_emit_ts2305() {
        let dir = std::env::temp_dir().join("tsz_check_bundler_dual_package_modes");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("node_modules/dual")).unwrap();

        let package_json_path = dir.join("node_modules/dual/package.json");
        let index_js_path = dir.join("node_modules/dual/index.js");
        let index_d_ts_path = dir.join("node_modules/dual/index.d.ts");
        let index_cjs_path = dir.join("node_modules/dual/index.cjs");
        let index_d_cts_path = dir.join("node_modules/dual/index.d.cts");
        let main_ts_path = dir.join("main.ts");
        let main_mts_path = dir.join("main.mts");
        let main_cts_path = dir.join("main.cts");

        fs::write(
            &package_json_path,
            r#"{
  "name": "dual",
  "version": "1.0.0",
  "type": "module",
  "main": "index.cjs",
  "types": "index.d.cts",
  "exports": {
    ".": {
      "import": "./index.js",
      "require": "./index.cjs"
    }
  }
}
"#,
        )
        .unwrap();
        fs::write(&index_js_path, "export const esm = 0;\n").unwrap();
        fs::write(&index_d_ts_path, "export const esm: number;\n").unwrap();
        fs::write(&index_cjs_path, "exports.cjs = 0;\n").unwrap();
        fs::write(&index_d_cts_path, "export const cjs: number;\n").unwrap();
        fs::write(&main_ts_path, "import { esm, cjs } from \"dual\";\n").unwrap();
        fs::write(&main_mts_path, "import { esm, cjs } from \"dual\";\n").unwrap();
        fs::write(&main_cts_path, "import { esm, cjs } from \"dual\";\n").unwrap();

        let options = ResolvedCompilerOptions {
            module_resolution: Some(crate::config::ModuleResolutionKind::Bundler),
            resolve_package_json_exports: true,
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::Preserve,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::Preserve,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (index_js_path.to_str().unwrap(), "export const esm = 0;\n"),
                (
                    index_d_ts_path.to_str().unwrap(),
                    "export const esm: number;\n",
                ),
                (index_cjs_path.to_str().unwrap(), "exports.cjs = 0;\n"),
                (
                    index_d_cts_path.to_str().unwrap(),
                    "export const cjs: number;\n",
                ),
                (
                    main_ts_path.to_str().unwrap(),
                    "import { esm, cjs } from \"dual\";\n",
                ),
                (
                    main_mts_path.to_str().unwrap(),
                    "import { esm, cjs } from \"dual\";\n",
                ),
                (
                    main_cts_path.to_str().unwrap(),
                    "import { esm, cjs } from \"dual\";\n",
                ),
            ],
            &options,
            &dir,
        );

        let import_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| {
                let file = Path::new(&diag.file);
                (file == main_ts_path.as_path()
                    || file == main_mts_path.as_path()
                    || file == main_cts_path.as_path())
                    && diag.code != 2318
            })
            .collect();

        assert!(
            import_diags.iter().all(|diag| diag.code != 2305),
            "expected no TS2305 for bundler dual-package import/require mode selection, got: {import_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn test_collect_diagnostics_preserve_symlinks_keeps_original_target_error() {
        use std::fs;
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("linked")).unwrap();
        fs::create_dir_all(dir.path().join("app/node_modules/real")).unwrap();
        fs::create_dir_all(dir.path().join("app/node_modules/linked")).unwrap();
        fs::create_dir_all(dir.path().join("app/node_modules/linked2")).unwrap();

        fs::write(
            dir.path().join("linked/index.d.ts"),
            "export { real } from \"real\";\nexport class C { private x; }\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("app/node_modules/real/index.d.ts"),
            "export const real: string;\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("app/app.ts"),
            "/// <reference types=\"linked\" />\nimport { C as C1 } from \"linked\";\nimport { C as C2 } from \"linked2\";\nlet x = new C1();\nx = new C2();\n",
        )
        .unwrap();
        symlink(
            dir.path().join("linked/index.d.ts"),
            dir.path().join("app/node_modules/linked/index.d.ts"),
        )
        .unwrap();
        symlink(
            dir.path().join("linked/index.d.ts"),
            dir.path().join("app/node_modules/linked2/index.d.ts"),
        )
        .unwrap();

        let resolved = ResolvedCompilerOptions {
            module_resolution: Some(crate::config::ModuleResolutionKind::Bundler),
            preserve_symlinks: true,
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::ES2015,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::ES2015,
                ..Default::default()
            },
            ..Default::default()
        };

        let file_paths = vec![
            dir.path().join("linked/index.d.ts"),
            dir.path().join("app/app.ts"),
        ];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let source_paths: FxHashSet<PathBuf> =
            sources.iter().map(|source| source.path.clone()).collect();
        assert!(source_paths.contains(&dir.path().join("linked/index.d.ts")));
        assert!(source_paths.contains(&dir.path().join("app/node_modules/linked/index.d.ts")));
        assert!(source_paths.contains(&dir.path().join("app/node_modules/linked2/index.d.ts")));

        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &[],
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &CheckerLibSet::default(),
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        assert!(
            diagnostics.iter().any(|diag| {
                diag.code
                    == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                    && diag.file.contains("linked/index.d.ts")
            }),
            "expected TS2307 for original linked target, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_mapped_type_generic_indexed_access_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"// repro from #49242

type Types = {
    [key: string]: object;
};

type Filled<T extends Types> = {
    [K in keyof T]: [T[K]];
}

class Test<Types extends {
    [key: string]: object;
}> {
    entries: {
        [T in keyof Types]?: Types[T][];
    } = {}

    get<T extends keyof Types>(name: T): Filled<Pick<Types, T>> {
        let entry = this.entries[name];
        if (entry) return { [name]: [entry[0]] } as Filled<Pick<Types, T>>;
        throw new Error("Entry not found");
    }
}

// repro from #49338

type TypesMap = {
    0: {
        foo: string,
    };
    1: {
        a: number,
    };
}
type P<T extends keyof TypesMap> = {
    t: T;
} & TypesMap[T];
type Handlers = { [M in keyof TypesMap]?: (p: P<M>) => void };
const typeHandlers: Handlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) => typeHandlers[p.t]?.(p);
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                        | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                )
            })
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected mapped generic indexed access repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_recursive_mapped_type_callback_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"type MorphTuple = [string, "|>", any]

type validateMorph<def extends MorphTuple> = def[1] extends "|>"
    ? [validateDefinition<def[0]>, "|>", (In: def[0]) => unknown]
    : def

type validateDefinition<def> = def extends MorphTuple
    ? validateMorph<def>
    : {
          [k in keyof def]: validateDefinition<def[k]>
      }

declare function type<def>(def: validateDefinition<def>): def

const shallow = type(["ark", "|>", (x) => x.length])
const objectLiteral = type({ a: ["ark", "|>", (x) => x.length] })
const nestedTuple = type([["ark", "|>", (x) => x.length]])
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                )
            })
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected recursive mapped-type callback repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_union_array_method_alias_callback_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }
interface Arr<T> {
  filter<S extends T>(pred: (value: T) => value is S): S[];
  filter(pred: (value: T) => unknown): T[];
}
declare const m: Arr<Fizz>["filter"] | Arr<Buzz>["filter"];
m(item => item.id < 5);
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected union overloaded array method alias repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_union_builtin_array_method_callback_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }

([] as Fizz[] | Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | readonly Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | Buzz[]).find(item => item);
([] as Fizz[] | Buzz[]).every(item => item.id < 5);
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected union built-in array method repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_reports_implicit_any_for_primitive_union_property_callback() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"type Validate = (text: string, pos: number, self: Rule) => number | boolean;
interface FullRule {
  validate: string | RegExp | Validate;
  normalize?: (match: {x: string}) => void;
}

type Rule = string | FullRule;

const obj: {field: Rule} = {
  field: {
    validate: (_t, _p, _s) => false,
    normalize: match => match.x,
  }
};
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_esnext_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
            .collect::<Vec<_>>();

        assert_eq!(
            relevant.len(),
            1,
            "Expected exactly one TS7006 for the primitive-union normalize callback, got: {diagnostics:?}"
        );
    }

    #[test]
    fn real_syntax_errors_suppress_cross_file_type_diagnostics() {
        let diagnostics = collect_test_diagnostics(&[
            ("/a.ts", "const x =\n"),
            ("/b.ts", "const y: number = \"s\";\n"),
        ]);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.file == "/a.ts" && diag.code == 1109),
            "expected the real syntax error to remain: {diagnostics:?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|diag| diag.file == "/b.ts" && diag.code == 2322),
            "did not expect TS2322 when another file has a real syntax error: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_reports_default_lib_breakage_from_global_node_merge() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
const enum SyntaxKind {
    Modifier,
    Decorator,
}

interface Node {
    kind: SyntaxKind;
}

interface Modifier extends Node { kind: SyntaxKind.Modifier; }
interface Decorator extends Node { kind: SyntaxKind.Decorator; }

declare function isModifier(node: Node): node is Modifier;
declare function isDecorator(node: Node): node is Decorator;

declare function every<T, U extends T>(array: readonly T[], callback: (element: T) => element is U): array is readonly U[];

declare const modifiers: readonly Decorator[] | readonly Modifier[];

function foo() {
    every(modifiers, isModifier);
    every(modifiers, isDecorator);
}
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        let lib_dom_diagnostics = diagnostics
            .iter()
            .filter(|diag| diag.file.ends_with("lib.dom.d.ts"))
            .collect::<Vec<_>>();
        let ts2344_count = lib_dom_diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT)
            .count();
        let ts2430_count = lib_dom_diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE)
            .count();

        // tsc reports three TS2344 diagnostics here: the apparent
        // `HTMLElementTagNameMap[K]` value union includes `HTMLTrackElement`,
        // whose existing `kind: string` property conflicts with the merged
        // `Node.kind: SyntaxKind` property.
        assert_eq!(
            ts2344_count, 3,
            "Expected three TS2344 diagnostics from lib.dom.d.ts after merging Node.kind, got: {diagnostics:?}"
        );
        assert_eq!(
            ts2430_count, 1,
            "Expected one TS2430 diagnostic from lib.dom.d.ts after merging Node.kind, got: {diagnostics:?}"
        );
    }

    #[test]
    fn default_lib_validation_ignores_unresolved_overload_cascades_after_global_merge() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
interface HTMLElement {
    type: string;
}
"#,
        );

        assert!(
            !diagnostics.iter().any(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
            }),
            "Did not expect default-lib TS2430 diagnostics from unrelated unresolved overload parameters, got: {diagnostics:?}"
        );
    }

    #[test]
    fn skip_lib_check_skips_default_lib_recheck_after_global_merge() {
        let diagnostics = collect_es2015_default_lib_diagnostics_with_options(
            r#"
const enum SyntaxKind {
    Modifier,
    Decorator,
}

interface Node {
    kind: SyntaxKind;
}

interface Modifier extends Node { kind: SyntaxKind.Modifier; }
interface Decorator extends Node { kind: SyntaxKind.Decorator; }

declare function isModifier(node: Node): node is Modifier;
declare function isDecorator(node: Node): node is Decorator;

declare function every<T, U extends T>(array: readonly T[], callback: (element: T) => element is U): array is readonly U[];

declare const modifiers: readonly Decorator[] | readonly Modifier[];

function foo() {
    every(modifiers, isModifier);
    every(modifiers, isDecorator);
}
"#,
            |resolved| {
                resolved.skip_lib_check = true;
            },
        );

        assert!(
            !diagnostics.iter().any(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && matches!(
                        diag.code,
                        diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                            | diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
                    )
            }),
            "Did not expect lib.dom.d.ts TS2344/TS2430 diagnostics when skipLibCheck is enabled, got: {diagnostics:?}"
        );
    }

    #[test]
    fn default_lib_validation_keeps_select_option_index_compatible_after_html_element_merge() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
declare global {
    interface ElementTagNameMap {
        [index: number]: HTMLElement
    }

    interface HTMLElement {
        [index: number]: HTMLElement;
    }
}

export {};
"#,
        );

        let lib_ts2430 = diagnostics
            .iter()
            .filter(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
            })
            .collect::<Vec<_>>();

        assert!(
            lib_ts2430
                .iter()
                .any(|diag| diag.message_text.contains("HTMLFormElement")),
            "Expected the real HTMLFormElement numeric-index incompatibility, got: {diagnostics:?}"
        );
        assert!(
            !lib_ts2430
                .iter()
                .any(|diag| diag.message_text.contains("HTMLSelectElement")),
            "Did not expect HTMLSelectElement to fail: its option/group index values inherit HTMLElement. Got: {diagnostics:?}"
        );
    }

    #[test]
    fn default_lib_validation_normalizes_cross_arena_method_members_after_global_merge() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
interface HTMLElement {
    clientWidth: number;
    isDisabled: boolean;
}

declare var document: Document;
interface Document {
    getElementById(elementId: string): HTMLElement;
}
"#,
        );

        assert!(
            !diagnostics.iter().any(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
            }),
            "Did not expect default-lib TS2430 diagnostics when a cross-arena method override is compatible, got: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_respects_skip_default_lib_check_for_global_node_merge() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
const enum SyntaxKind {
    Modifier,
    Decorator,
}

interface Node {
    kind: SyntaxKind;
}
"#,
        )
        .expect("write source");

        let mut resolved = resolved_options_for_es2015_strict_test();
        resolved.skip_default_lib_check = true;
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            type_reference_errors,
            resolution_mode_errors,
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &program,
            &resolved,
            dir.path(),
            None,
            &checker_libs,
            (false, false, false),
            &type_cache_output,
            false,
            false,
        )
        .diagnostics;

        assert!(
            !diagnostics.iter().any(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && matches!(
                        diag.code,
                        diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                            | diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
                    )
            }),
            "Did not expect lib.dom.d.ts TS2344/TS2430 diagnostics when skipDefaultLibCheck is enabled, got: {diagnostics:?}"
        );
    }

    #[test]
    fn real_syntax_errors_preserve_checker_grammar_diagnostics() {
        let diagnostics = collect_test_diagnostics(&[
            ("/a.ts", "const x =\n"),
            ("/b.ts", "type void = string;\n"),
        ]);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.file == "/b.ts" && diag.code == 2457),
            "expected TS2457 to survive program-level syntax suppression: {diagnostics:?}"
        );
    }

    #[test]
    fn tarjan_scc_no_edges() {
        let adj = vec![vec![], vec![], vec![]];
        let sccs = tarjan_scc(3, &adj);
        // Each node is its own SCC
        assert_eq!(sccs.len(), 3);
        for scc in &sccs {
            assert_eq!(scc.len(), 1);
        }
    }

    #[test]
    fn tarjan_scc_linear_chain() {
        // 0 -> 1 -> 2 (no cycles)
        let adj = vec![vec![1], vec![2], vec![]];
        let sccs = tarjan_scc(3, &adj);
        assert_eq!(sccs.len(), 3);
        for scc in &sccs {
            assert_eq!(scc.len(), 1);
        }
    }

    #[test]
    fn tarjan_scc_simple_cycle() {
        // 0 -> 1 -> 0 (one cycle of size 2)
        let adj = vec![vec![1], vec![0]];
        let sccs = tarjan_scc(2, &adj);
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].len(), 2);
    }

    #[test]
    fn tarjan_scc_triangle_cycle() {
        // 0 -> 1 -> 2 -> 0 (one cycle of size 3)
        let adj = vec![vec![1], vec![2], vec![0]];
        let sccs = tarjan_scc(3, &adj);
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].len(), 3);
    }

    #[test]
    fn tarjan_scc_mixed() {
        // 0 -> 1 -> 2 -> 1 (cycle {1,2}), 3 standalone
        let adj = vec![vec![1], vec![2], vec![1], vec![]];
        let sccs = tarjan_scc(4, &adj);
        let cycles: Vec<_> = sccs.iter().filter(|s| s.len() > 1).collect();
        assert_eq!(cycles.len(), 1, "expected exactly one cycle");
        assert_eq!(cycles[0].len(), 2, "cycle should have 2 nodes");
    }

    #[test]
    fn real_syntax_errors_preserve_reserved_interface_name_diagnostics() {
        let diagnostics = collect_test_diagnostics(&[
            ("/a.ts", "const x =\n"),
            ("/b.ts", "function function() {}\ninterface void {}\n"),
        ]);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.file == "/b.ts" && diag.code == 2427),
            "expected TS2427 to survive parse-error suppression: {diagnostics:?}"
        );
    }

    // --- topological_file_order tests ---

    #[test]
    fn topo_order_empty() {
        let result = topological_file_order(&[], &FxHashMap::default());
        assert!(result.is_empty());
    }

    #[test]
    fn topo_order_single_file() {
        let result = topological_file_order(&[0], &FxHashMap::default());
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn topo_order_no_deps() {
        // Three files with no dependencies — output should be sorted by index
        let result = topological_file_order(&[2, 0, 1], &FxHashMap::default());
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn topo_order_linear_chain() {
        // File 0 imports file 1, file 1 imports file 2
        // Expected order: 2 (no deps), then 1, then 0
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./c".to_string()), 2);

        let result = topological_file_order(&[0, 1, 2], &deps);
        assert_eq!(result, vec![2, 1, 0]);
    }

    #[test]
    fn topo_order_diamond() {
        // File 0 imports 1 and 2; both 1 and 2 import 3
        // Expected: 3 first, then 1 and 2 (sorted), then 0
        let mut deps = FxHashMap::default();
        deps.insert((0, "./a".to_string()), 1);
        deps.insert((0, "./b".to_string()), 2);
        deps.insert((1, "./c".to_string()), 3);
        deps.insert((2, "./c".to_string()), 3);

        let result = topological_file_order(&[0, 1, 2, 3], &deps);
        assert_eq!(result, vec![3, 1, 2, 0]);
    }

    #[test]
    fn topo_order_cycle() {
        // Circular: 0 -> 1 -> 0
        // Both participate in a cycle; should still include both files
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./a".to_string()), 0);

        let result = topological_file_order(&[0, 1], &deps);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&0));
        assert!(result.contains(&1));
    }

    #[test]
    fn topo_order_partial_cycle() {
        // File 2 has no deps; files 0 and 1 form a cycle
        // Expected: 2 first (no deps), then 0, 1 (cycle participants appended)
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./a".to_string()), 0);

        let result = topological_file_order(&[0, 1, 2], &deps);
        assert_eq!(result[0], 2, "dependency-free file should come first");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn topo_order_ignores_external_deps() {
        // File 0 depends on file 5, but 5 is not in file_indices — should be ignored
        let mut deps = FxHashMap::default();
        deps.insert((0, "./ext".to_string()), 5);

        let result = topological_file_order(&[0, 1], &deps);
        assert_eq!(result.len(), 2);
        // Both have no in-set dependencies, so sorted order
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn topo_order_self_import_ignored() {
        // File 0 imports itself — self-loops should be ignored
        let mut deps = FxHashMap::default();
        deps.insert((0, "./self".to_string()), 0);

        let result = topological_file_order(&[0, 1], &deps);
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn ts6504_emitted_for_js_root_when_allow_js_disabled() {
        // When allowJs is not set, an explicit JS root must produce TS6504.
        // tsc includes the file in the program but reports the error and skips
        // semantic checks for that file.
        let options = ResolvedCompilerOptions {
            allow_js: false,
            ..ResolvedCompilerOptions::default()
        };
        let diagnostics = collect_test_diagnostics_with_options(
            &[("/main.js", "const n = 1;\n")],
            &options,
            std::path::Path::new("/"),
        );

        assert!(
            diagnostics.iter().any(|d| d.code == 6504),
            "expected TS6504 for JS root without allowJs, got: {diagnostics:?}"
        );

        let ts6504 = diagnostics.iter().find(|d| d.code == 6504).unwrap();
        assert!(
            ts6504.message_text.contains("main.js"),
            "TS6504 message should include the JS file path: {}",
            ts6504.message_text
        );
        assert!(
            ts6504.related_information.len() >= 2,
            "TS6504 should have related info explaining why the file is in the program"
        );
    }

    #[test]
    fn ts6504_not_emitted_when_allow_js_enabled() {
        // When allowJs is enabled, JS root files are accepted without TS6504.
        let options = ResolvedCompilerOptions {
            allow_js: true,
            ..ResolvedCompilerOptions::default()
        };
        let diagnostics = collect_test_diagnostics_with_options(
            &[("/main.js", "const n = 1;\n")],
            &options,
            std::path::Path::new("/"),
        );

        assert!(
            !diagnostics.iter().any(|d| d.code == 6504),
            "expected no TS6504 when allowJs is enabled, got: {diagnostics:?}"
        );
    }
}
