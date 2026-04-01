//! Core type checking logic for tsz-server.
//!
//! Contains the check pipeline: semantic diagnostics, `run_check`, lib loading,
//! and checker option construction.

use super::{CheckOptions, Server, TsServerRequest, TsServerResponse};
use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz::binder::BinderState;
use tsz::checker::context::{CheckerOptions, LibContext, ProjectEnv};
use tsz::checker::diagnostics::DiagnosticCategory;
use tsz::checker::module_resolution::build_module_resolution_maps;
use tsz::checker::state::CheckerState;
use tsz::emitter::ScriptTarget;
use tsz::lib_loader::LibFile;
use tsz::parallel;
use tsz::parser::ParserState;
use tsz::parser::node::NodeArena;
use tsz_cli::config::{
    checker_target_from_emitter, default_lib_name_for_target, resolve_default_lib_files_from_dir,
    resolve_lib_files_from_dir,
};
use tsz_solver::QueryCache;
use tsz_solver::RelationCacheStats;

pub(crate) struct RunCheckResult {
    pub(crate) codes: Vec<i32>,
    pub(crate) relation_cache_stats: RelationCacheStats,
}

impl Server {
    /// Get full semantic diagnostics for a single file (with position info).
    pub(crate) fn get_semantic_diagnostics_full(
        &mut self,
        file_path: &str,
        content: &str,
    ) -> Vec<tsz::checker::diagnostics::Diagnostic> {
        self.get_diagnostics_by_category(file_path, content, DiagnosticCategory::Error)
    }

    /// Get suggestion diagnostics for a single file.
    pub(crate) fn get_suggestion_diagnostics(
        &mut self,
        file_path: &str,
        content: &str,
    ) -> Vec<tsz::checker::diagnostics::Diagnostic> {
        self.get_diagnostics_by_category(file_path, content, DiagnosticCategory::Suggestion)
    }

    fn get_diagnostics_by_category(
        &mut self,
        file_path: &str,
        content: &str,
        category: DiagnosticCategory,
    ) -> Vec<tsz::checker::diagnostics::Diagnostic> {
        let mut options = self.inferred_check_options.clone();
        if (self.inferred_module_is_none_for_projects
            && !self.auto_imports_allowed_for_inferred_projects)
            || self.fourslash_module_none_directive_blocks_import_syntax(file_path)
        {
            options.module = Some("none".to_string());
        }

        let binding_lib_files = match if options.no_lib {
            Ok(vec![])
        } else {
            self.load_libs_for_binding(&options)
        } {
            Ok(libs) => libs,
            Err(_) => return Vec::new(),
        };
        let checker_lib_files = match if options.no_lib {
            Ok(vec![])
        } else {
            self.load_libs_unified(&options)
        } {
            Ok(libs) => libs,
            Err(_) => return Vec::new(),
        };

        let mut files: Vec<(String, String)> = self
            .open_files
            .iter()
            .map(|(path, raw)| {
                (
                    path.clone(),
                    Self::normalize_fourslash_virtual_content(path, raw),
                )
            })
            .collect();
        if let Some((_, existing)) = files.iter_mut().find(|(path, _)| path == file_path) {
            *existing = content.to_string();
        } else {
            files.push((file_path.to_string(), content.to_string()));
        }
        files.retain(|(path, _)| Self::is_checkable_file(path));

        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            files,
            &binding_lib_files,
        ));
        let checker_options = self.build_checker_options(&options);
        let lib_contexts: Vec<LibContext> = checker_lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        let query_cache = QueryCache::new(&program.type_interner);

        let all_arenas: Arc<Vec<Arc<NodeArena>>> = Arc::new(
            program
                .files
                .iter()
                .map(|file| Arc::clone(&file.arena))
                .collect(),
        );
        let all_binders: Arc<Vec<Arc<BinderState>>> = Arc::new(
            program
                .files
                .iter()
                .enumerate()
                .map(|(file_idx, file)| {
                    Arc::new(parallel::create_binder_from_bound_file(
                        file, &program, file_idx,
                    ))
                })
                .collect(),
        );
        let file_names: Vec<String> = program
            .files
            .iter()
            .map(|file| file.file_name.clone())
            .collect();
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
        let resolved_modules_arc = Arc::new(resolved_modules);

        // Build skeleton indices if available
        let (skeleton_declared_modules, skeleton_expando_index) = if let Some(ref skel) =
            program.skeleton_index
        {
            let (exact, patterns) = skel.build_declared_module_sets();
            (
                Some(Arc::new(
                    tsz::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns),
                )),
                Some(Arc::new(skel.expando_properties.clone())),
            )
        } else {
            (None, None)
        };

        let mut project_env = ProjectEnv {
            lib_contexts: std::sync::Arc::new(lib_contexts),
            all_arenas,
            all_binders,
            skeleton_declared_modules,
            skeleton_expando_index,
            resolved_module_paths: Arc::new(resolved_module_paths),
            ..Default::default()
        };
        project_env.build_global_indices();

        let mut diagnostics: Vec<tsz::checker::diagnostics::Diagnostic> = Vec::new();
        for (file_idx, file) in program.files.iter().enumerate() {
            if category == DiagnosticCategory::Error && file.file_name == file_path {
                diagnostics.extend(file.parse_diagnostics.iter().map(|diag| {
                    tsz::checker::diagnostics::Diagnostic::error(
                        file.file_name.clone(),
                        diag.start,
                        diag.length,
                        diag.message.clone(),
                        diag.code,
                    )
                }));
            }

            let mut checker = CheckerState::new(
                &file.arena,
                &project_env.all_binders[file_idx],
                &query_cache,
                file.file_name.clone(),
                checker_options.clone(),
            );

            project_env.apply_to(&mut checker.ctx);
            checker
                .ctx
                .set_resolved_modules((*resolved_modules_arc).clone());
            checker.ctx.set_current_file_idx(file_idx);
            checker.check_source_file(file.source_file);

            if file.file_name == file_path {
                diagnostics.extend(
                    checker
                        .ctx
                        .diagnostics
                        .into_iter()
                        .filter(|diag| diag.category == category),
                );
            }
        }

        if category == DiagnosticCategory::Error {
            diagnostics
                .retain(|diag| !Self::should_suppress_namespace_global_ts2403(diag, content));
        }

        diagnostics
    }

    fn fourslash_module_none_directive_blocks_import_syntax(&self, file_path: &str) -> bool {
        self.open_files
            .get(file_path)
            .and_then(|text| Self::fourslash_module_none_blocking_imports_from_text(text))
            .or_else(|| {
                self.open_files.iter().find_map(|(path, text)| {
                    if path == file_path {
                        return None;
                    }
                    Self::fourslash_module_none_blocking_imports_from_text(text)
                })
            })
            .unwrap_or(false)
    }

    fn fourslash_module_none_blocking_imports_from_text(source_text: &str) -> Option<bool> {
        let mut saw_module = false;
        let mut module_none = false;
        let mut saw_target = false;
        let mut target_supports_imports = false;

        for line in source_text.lines().take(64) {
            let trimmed = line.trim_start();
            let directive = trimmed.trim_start_matches('/').trim_start();
            if let Some(rest) = directive.strip_prefix("@module:") {
                saw_module = true;
                module_none = rest.split(',').map(str::trim).any(|value| {
                    value.eq_ignore_ascii_case("none") || value.parse::<i64>().ok() == Some(0)
                });
                continue;
            }

            if let Some(rest) = directive.strip_prefix("@target:") {
                saw_target = true;
                target_supports_imports = rest.split(',').map(str::trim).any(|value| {
                    value.eq_ignore_ascii_case("es6")
                        || value.eq_ignore_ascii_case("es2015")
                        || value.eq_ignore_ascii_case("es2016")
                        || value.eq_ignore_ascii_case("es2017")
                        || value.eq_ignore_ascii_case("es2018")
                        || value.eq_ignore_ascii_case("es2019")
                        || value.eq_ignore_ascii_case("es2020")
                        || value.eq_ignore_ascii_case("es2021")
                        || value.eq_ignore_ascii_case("es2022")
                        || value.eq_ignore_ascii_case("es2023")
                        || value.eq_ignore_ascii_case("es2024")
                        || value.eq_ignore_ascii_case("esnext")
                        || value.eq_ignore_ascii_case("latest")
                        || value.parse::<i64>().ok().is_some_and(|n| n >= 2)
                });
            }
        }

        if saw_module && module_none {
            return Some(!(saw_target && target_supports_imports));
        }
        None
    }

    fn should_suppress_namespace_global_ts2403(
        diag: &tsz::checker::diagnostics::Diagnostic,
        content: &str,
    ) -> bool {
        if diag.code
            != tsz::checker::diagnostics::diagnostic_codes::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_MUST_BE_OF_TYP
        {
            return false;
        }

        let marker = "Variable '";
        let Some(start) = diag.message_text.find(marker) else {
            return false;
        };
        let tail = &diag.message_text[start + marker.len()..];
        let Some(end) = tail.find('\'') else {
            return false;
        };
        let name = &tail[..end];
        if name.is_empty() {
            return false;
        }

        let has_ambient_namespace = content.contains("declare namespace");
        let has_namespace_var = content.contains(&format!("var {name}:"));
        let has_global_decl = content.contains(&format!("declare var {name}:"));
        has_ambient_namespace && has_namespace_var && has_global_decl
    }

    pub(crate) fn handle_tsz_performance(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let body = (|| -> Option<serde_json::Value> {
            let file = request
                .arguments
                .get("file")
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string)
                .or_else(|| self.open_files.keys().next().cloned())?;

            let content = request
                .arguments
                .get("fileContent")
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string)
                .or_else(|| self.open_files.get(&file).cloned())
                .or_else(|| std::fs::read_to_string(&file).ok())?;

            let mut files = FxHashMap::default();
            files.insert(file.clone(), content);
            let result = self.run_check(files, CheckOptions::default()).ok()?;
            let stats = result.relation_cache_stats;

            let mut payload = serde_json::json!({
                "file": file,
                "checksCompleted": self.checks_completed,
                "errorCount": result.codes.len(),
                "errorCodes": result.codes,
                "relationCache": {
                    "subtypeHits": stats.subtype_hits,
                    "subtypeMisses": stats.subtype_misses,
                    "subtypeEntries": stats.subtype_entries,
                    "assignabilityHits": stats.assignability_hits,
                    "assignabilityMisses": stats.assignability_misses,
                    "assignabilityEntries": stats.assignability_entries
                }
            });

            if self.enable_telemetry {
                payload["telemetryEvent"] = serde_json::json!({
                    "eventName": "tszPerformance",
                    "relationCache": payload["relationCache"].clone()
                });
            }

            Some(payload)
        })();

        self.stub_response(seq, request, body)
    }

    pub(crate) fn run_check(
        &mut self,
        files: FxHashMap<String, String>,
        options: CheckOptions,
    ) -> Result<RunCheckResult> {
        // Two-phase lib loading (matches get_semantic_diagnostics_full):
        // 1. Individual lib files for binding (proper symbol resolution per-lib)
        // 2. Unified merged lib for checker (cross-lib type resolution)
        let binding_lib_files = if options.no_lib {
            vec![]
        } else {
            self.load_libs_for_binding(&options)?
        };
        let checker_lib_files = if options.no_lib {
            vec![]
        } else {
            self.load_libs_unified(&options)?
        };

        let checker_options = self.build_checker_options(&options);

        // Filter checkable files and detect binary content
        let mut checkable_files: Vec<(String, String)> = Vec::with_capacity(files.len());
        let mut binary_file_errors: Vec<(String, i32)> = Vec::new();

        for (file_name, content) in files {
            if !Self::is_checkable_file(&file_name) {
                continue;
            }
            if super::content_appears_binary(&content) {
                binary_file_errors
                    .push((file_name.clone(), super::TS1490_FILE_APPEARS_TO_BE_BINARY));
                continue;
            }
            checkable_files.push((file_name, content));
        }

        // Use the same parse+bind pipeline as get_semantic_diagnostics_full:
        // parallel::parse_and_bind_parallel_with_libs properly integrates lib
        // symbols during binding, resolving cross-lib references.
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            checkable_files,
            &binding_lib_files,
        ));

        let lib_contexts: Vec<LibContext> = checker_lib_files
            .iter()
            .map(|lib| LibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();

        let all_arenas: Arc<Vec<Arc<NodeArena>>> = Arc::new(
            program
                .files
                .iter()
                .map(|file| Arc::clone(&file.arena))
                .collect(),
        );
        let all_binders: Arc<Vec<Arc<BinderState>>> = Arc::new(
            program
                .files
                .iter()
                .enumerate()
                .map(|(file_idx, file)| {
                    Arc::new(parallel::create_binder_from_bound_file(
                        file, &program, file_idx,
                    ))
                })
                .collect(),
        );

        let file_names: Vec<String> = program
            .files
            .iter()
            .map(|file| file.file_name.clone())
            .collect();
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
        let resolved_modules_arc = Arc::new(resolved_modules);

        // Build skeleton indices if available
        let (skeleton_declared_modules, skeleton_expando_index) = if let Some(ref skel) =
            program.skeleton_index
        {
            let (exact, patterns) = skel.build_declared_module_sets();
            (
                Some(Arc::new(
                    tsz::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns),
                )),
                Some(Arc::new(skel.expando_properties.clone())),
            )
        } else {
            (None, None)
        };

        let mut project_env = ProjectEnv {
            lib_contexts: std::sync::Arc::new(lib_contexts),
            all_arenas,
            all_binders,
            skeleton_declared_modules,
            skeleton_expando_index,
            resolved_module_paths: Arc::new(resolved_module_paths),
            ..Default::default()
        };
        project_env.build_global_indices();

        // Type check all files
        let query_cache = QueryCache::new(&program.type_interner);
        let mut all_codes: Vec<i32> = Vec::new();

        // Add TS1490 for binary files detected earlier
        for (_file_name, code) in binary_file_errors {
            all_codes.push(code);
        }

        for (file_idx, file) in program.files.iter().enumerate() {
            // Include parse errors
            all_codes.extend(file.parse_diagnostics.iter().map(|d| d.code as i32));

            let mut checker = CheckerState::new(
                &file.arena,
                &project_env.all_binders[file_idx],
                &query_cache,
                file.file_name.clone(),
                checker_options.clone(),
            );

            project_env.apply_to(&mut checker.ctx);
            checker
                .ctx
                .set_resolved_modules((*resolved_modules_arc).clone());
            checker.ctx.set_current_file_idx(file_idx);
            checker.check_source_file(file.source_file);

            for diag in &checker.ctx.diagnostics {
                if diag.category == DiagnosticCategory::Error {
                    all_codes.push(diag.code as i32);
                }
            }
        }

        Ok(RunCheckResult {
            codes: all_codes,
            relation_cache_stats: query_cache.relation_cache_stats(),
        })
    }

    /// Load libs with unified symbol merging.
    ///
    /// This method implements **cumulative binding** to solve cross-lib symbol resolution:
    /// 1. Loads libs in dependency order using the old method (each with its own binder)
    /// 2. Creates a unified merged binder from all lib binders
    /// 3. Returns a single `LibFile` with the merged binder
    ///
    /// This allows proper cross-lib type resolution (e.g., `Array` from es5 visible in dom).
    pub(crate) fn load_libs_for_binding(
        &mut self,
        options: &CheckOptions,
    ) -> Result<Vec<Arc<LibFile>>> {
        let lib_paths = if let Some(ref libs) = options.lib {
            resolve_lib_files_from_dir(libs, &self.lib_dir)
        } else {
            resolve_default_lib_files_from_dir(Self::parse_target(&options.target), &self.lib_dir)
        };
        let lib_paths = match lib_paths {
            Ok(paths) => paths,
            Err(_) => {
                let lib_names = self.determine_libs(options);
                if lib_names.is_empty() {
                    return Ok(vec![]);
                }

                let mut lib_files = Vec::new();
                let mut loaded = rustc_hash::FxHashSet::default();
                for lib_name in &lib_names {
                    self.load_lib_recursive(lib_name, &mut lib_files, &mut loaded)?;
                }
                return Ok(lib_files);
            }
        };
        if lib_paths.is_empty() {
            return Ok(vec![]);
        }

        let lib_refs: Vec<&std::path::Path> = lib_paths.iter().map(|path| path.as_path()).collect();
        parallel::load_lib_files_for_binding_strict(&lib_refs)
    }

    pub(crate) fn load_libs_unified(
        &mut self,
        options: &CheckOptions,
    ) -> Result<Vec<Arc<LibFile>>> {
        let mut lib_names = self.determine_libs(options);
        if lib_names.is_empty() {
            return Ok(vec![]);
        }

        // Sort for deterministic cache key
        lib_names.sort();

        // Check cache first
        if let Some((cached_names, cached_lib)) = &self.unified_lib_cache
            && *cached_names == lib_names
        {
            return Ok(vec![std::sync::Arc::clone(cached_lib)]);
        }

        // Phase 1: Load all libs normally (each with its own binder)
        let mut lib_files = Vec::new();
        let mut loaded = rustc_hash::FxHashSet::default();
        for lib_name in &lib_names {
            self.load_lib_recursive(lib_name, &mut lib_files, &mut loaded)?;
        }

        if lib_files.is_empty() {
            return Ok(vec![]);
        }

        // Phase 2: Create LibContexts from all loaded libs
        // Use binder::LibContext for merge_lib_contexts_into_binder
        use tsz::binder::LibContext as BinderLibContext;
        let lib_contexts: Vec<BinderLibContext> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();

        // Phase 3: Create a unified binder by merging ALL lib binders
        // This is the key fix - we merge symbols from all libs into a single binder
        // so cross-lib references (e.g., Array in dom.d.ts from es5.d.ts) are resolved
        let mut unified_binder = BinderState::new();
        unified_binder.merge_lib_contexts_into_binder(&lib_contexts);

        // Phase 4: Create a unified LibFile
        // Use the first arena as representative (the unified binder tracks symbol_arenas)
        let unified_arena = lib_files.first().map_or_else(
            || Arc::new(tsz::parser::node::NodeArena::new()),
            |lib| Arc::clone(&lib.arena),
        );

        let unified_root = lib_files
            .first()
            .map(|f| f.root_index)
            .unwrap_or(tsz::parser::NodeIndex(0));
        let unified_lib = Arc::new(LibFile::new(
            "unified-libs".to_string(),
            unified_arena,
            Arc::new(unified_binder),
            unified_root,
        ));

        // Cache the result
        self.unified_lib_cache = Some((lib_names, std::sync::Arc::clone(&unified_lib)));

        Ok(vec![unified_lib])
    }

    /// Normalize lib name aliases to their canonical form.
    /// IMPORTANT: lib.es6.d.ts and lib.es2015.d.ts are DIFFERENT files:
    /// - lib.es6.d.ts includes ES2015 + DOM (for web targets)
    /// - lib.es2015.d.ts includes ES2015 ONLY (for non-web targets)
    ///
    /// For conformance tests (web targets), we MUST use "es6" to get DOM types.
    /// TypeScript's libMap (in commandLineParser.ts) maps short names to actual lib files.
    ///
    /// lib name mapping:
    /// - es6 -> es6 (NOT es2015! lib.es6.d.ts includes DOM)
    /// - es7 -> es2016
    /// - lib -> es5 (the default lib.d.ts)
    /// - dom -> dom.generated (TypeScript source uses .generated suffix)
    pub(crate) fn normalize_lib_alias(name: &str) -> String {
        match name.to_lowercase().trim() {
            // ES version aliases
            // NOTE: Do NOT map "es6" to "es2015" - they are different files!
            // lib.es6.d.ts = ES2015 + DOM (web)
            // lib.es2015.d.ts = ES2015 only (non-web)
            "es6" => "es6".to_string(),
            "es7" => "es2016".to_string(),
            // lib.d.ts is equivalent to es5
            "lib" | "lib.d.ts" => "es5".to_string(),
            // DOM aliases - TypeScript source uses .generated suffix
            "dom" => "dom.generated".to_string(),
            "dom.iterable" => "dom.iterable.generated".to_string(),
            "dom.asynciterable" => "dom.asynciterable.generated".to_string(),
            // Full lib aliases (e.g., "lib.es6.d.ts" -> "es6")
            s if s.starts_with("lib.") && s.ends_with(".d.ts") => {
                let inner = &s[4..s.len() - 5]; // Extract between "lib." and ".d.ts"
                Self::normalize_lib_alias(inner)
            }
            // Pass through others unchanged
            other => other.to_string(),
        }
    }

    pub(crate) fn load_lib_recursive(
        &mut self,
        lib_name: &str,
        result: &mut Vec<Arc<LibFile>>,
        loaded: &mut rustc_hash::FxHashSet<String>,
    ) -> Result<()> {
        // Apply lib aliasing (es6 -> es2015, es7 -> es2016, etc.)
        let aliased = Self::normalize_lib_alias(lib_name);
        let normalized = aliased.trim().to_lowercase();
        if loaded.contains(&normalized) {
            return Ok(());
        }
        loaded.insert(normalized.clone());

        if let Some((lib, references)) = self.lib_cache.get(&normalized) {
            let lib_clone = std::sync::Arc::clone(lib);
            let refs = references.clone();
            for ref_lib in &refs {
                self.load_lib_recursive(ref_lib, result, loaded)?;
            }
            result.push(lib_clone);
            return Ok(());
        }

        let candidates = [
            self.lib_dir.join(format!("{normalized}.d.ts")),
            self.lib_dir.join(format!("lib.{normalized}.d.ts")),
            self.tests_lib_dir.join(format!("{normalized}.d.ts")),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                let content = std::fs::read_to_string(candidate)
                    .with_context(|| format!("failed to read lib file: {}", candidate.display()))?;
                let references = Self::parse_lib_references(&content);
                for ref_lib in &references {
                    self.load_lib_recursive(ref_lib, result, loaded)?;
                }

                let file_name = candidate.file_name().map_or_else(
                    || format!("lib.{normalized}.d.ts"),
                    |s| s.to_string_lossy().to_string(),
                );
                let mut parser = ParserState::new(file_name.clone(), content);
                let root_idx = parser.parse_source_file();
                let mut binder = BinderState::new();
                binder.bind_source_file(parser.get_arena(), root_idx);

                let lib = Arc::new(LibFile::new(
                    file_name,
                    Arc::new(parser.into_arena()),
                    Arc::new(binder),
                    root_idx,
                ));

                // Cap lib_cache size to prevent unbounded growth
                const MAX_LIB_CACHE_ENTRIES: usize = 50;
                if self.lib_cache.len() >= MAX_LIB_CACHE_ENTRIES {
                    // Clear cache if it gets too large
                    self.lib_cache.clear();
                    // Also clear unified cache as it depends on lib_cache
                    self.unified_lib_cache = None;
                }

                self.lib_cache.insert(
                    normalized,
                    (std::sync::Arc::clone(&lib), references.clone()),
                );
                result.push(lib);
                return Ok(());
            }
        }

        // No embedded libs fallback - lib files must be on disk (matching tsgo behavior)
        // Users need TypeScript installed or TSZ_LIB_DIR set
        Ok(())
    }

    pub(crate) fn parse_lib_references(content: &str) -> Vec<String> {
        let mut refs = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("///") {
                continue;
            }
            if let Some(start) = trimmed.find("<reference") {
                let rest = &trimmed[start..];
                if let Some(lib_start) = rest.find("lib=") {
                    let after_lib = &rest[lib_start + 4..];
                    let quote = after_lib.chars().next();
                    if quote == Some('"') || quote == Some('\'') {
                        let quote_char = quote
                            .expect("guarded by quote == Some('\"') || quote == Some('\\'') check");
                        let value_start = 1;
                        if let Some(end) = after_lib[value_start..].find(quote_char) {
                            let lib_name = &after_lib[value_start..value_start + end];
                            refs.push(lib_name.trim().to_lowercase());
                        }
                    }
                }
            }
        }
        refs
    }

    pub(crate) fn determine_libs(&self, options: &CheckOptions) -> Vec<String> {
        if options.no_lib {
            return vec![];
        }
        if let Some(ref libs) = options.lib {
            libs.iter().map(|s| s.trim().to_lowercase()).collect()
        } else {
            let target = Self::parse_target(&options.target);
            let default_lib = default_lib_name_for_target(target);
            vec![default_lib.to_string()]
        }
    }

    /// Returns true if the file has a TypeScript or JavaScript extension that
    /// should be parsed and type-checked. Non-source files (.json, .txt, etc.)
    /// that appear in multi-file test fixtures should be skipped.
    pub(crate) fn is_checkable_file(file_name: &str) -> bool {
        let lower = file_name.to_lowercase();
        // Order: most common extensions first for early return
        lower.ends_with(".ts")
            || lower.ends_with(".tsx")
            || lower.ends_with(".js")
            || lower.ends_with(".jsx")
            || lower.ends_with(".mts")
            || lower.ends_with(".cts")
            || lower.ends_with(".mjs")
            || lower.ends_with(".cjs")
    }

    pub(crate) fn parse_target(target: &Option<String>) -> ScriptTarget {
        target.as_ref().map_or(ScriptTarget::ES5, |t| {
            // Handle comma-separated targets (e.g., "es2015,es2017") by taking the first one
            // This matches how TSC runs multi-target tests - one iteration per target
            let first_target = t.split(',').next().unwrap_or(t).trim().to_lowercase();
            match first_target.as_str() {
                "es3" => ScriptTarget::ES3,
                "es5" => ScriptTarget::ES5,
                "es6" | "es2015" => ScriptTarget::ES2015,
                "es2016" => ScriptTarget::ES2016,
                "es2017" => ScriptTarget::ES2017,
                "es2018" => ScriptTarget::ES2018,
                "es2019" => ScriptTarget::ES2019,
                "es2020" => ScriptTarget::ES2020,
                "es2021" => ScriptTarget::ES2021,
                "es2022" | "es2023" => ScriptTarget::ES2022,
                _ => ScriptTarget::ESNext,
            }
        })
    }

    pub(crate) fn build_checker_options(&self, options: &CheckOptions) -> CheckerOptions {
        let emitter_target = Self::parse_target(&options.target);
        let checker_target = checker_target_from_emitter(emitter_target);

        // Start with CheckerOptions::default() which has TypeScript 6.0 defaults
        // (strict: true and all strict-family flags enabled)
        // Then apply explicit overrides from the request options
        let defaults = CheckerOptions::default();

        CheckerOptions {
            strict: options.strict,
            strict_null_checks: options.strict_null_checks.unwrap_or(options.strict || defaults.strict_null_checks),
            strict_function_types: options.strict_function_types.unwrap_or(options.strict || defaults.strict_function_types),
            strict_bind_call_apply: options.strict_bind_call_apply.unwrap_or(options.strict || defaults.strict_bind_call_apply),
            strict_property_initialization: options
                .strict_property_initialization
                .unwrap_or(options.strict || defaults.strict_property_initialization),
            no_implicit_any: options.no_implicit_any.unwrap_or(options.strict || defaults.no_implicit_any),
            no_implicit_this: options.no_implicit_this.unwrap_or(options.strict || defaults.no_implicit_this),
            no_implicit_returns: options.no_implicit_returns,
            exact_optional_property_types: options.exact_optional_property_types,
            no_unchecked_indexed_access: options.no_unchecked_indexed_access,
            use_unknown_in_catch_variables: options
                .use_unknown_in_catch_variables
                .unwrap_or(options.strict || defaults.use_unknown_in_catch_variables),
            isolated_modules: options.isolated_modules,
            no_lib: options.no_lib,
            no_types_and_symbols: false,
            target: checker_target,
            module: if let Some(module_str) = &options.module {
                // Parse module kind from string (inline version of parse_module_kind)
                match module_str.to_lowercase().as_str() {
                    "commonjs" => tsz::ModuleKind::CommonJS,
                    "amd" => tsz::ModuleKind::AMD,
                    "umd" => tsz::ModuleKind::UMD,
                    "system" => tsz::ModuleKind::System,
                    "es2015" => tsz::ModuleKind::ES2015,
                    "es2020" => tsz::ModuleKind::ES2020,
                    "es2022" => tsz::ModuleKind::ES2022,
                    "esnext" => tsz::ModuleKind::ESNext,
                    "node16" => tsz::ModuleKind::Node16,
                    "node18" => tsz::ModuleKind::Node18,
                    "node20" => tsz::ModuleKind::Node20,
                    "nodenext" => tsz::ModuleKind::NodeNext,
                    _ => tsz::ModuleKind::None,
                }
            } else {
                // Default to CommonJS if not specified (matches tsc behavior)
                tsz::ModuleKind::CommonJS
            },
            es_module_interop: options.es_module_interop,
            allow_synthetic_default_imports: options
                .allow_synthetic_default_imports
                .unwrap_or(options.es_module_interop || defaults.allow_synthetic_default_imports),
            allow_unreachable_code: options.allow_unreachable_code,
            allow_unused_labels: options.allow_unused_labels,
            no_property_access_from_index_signature: options
                .no_property_access_from_index_signature,
            sound_mode: false, // Sound mode not yet exposed in server protocol
            experimental_decorators: options.experimental_decorators,
            no_unused_locals: options.no_unused_locals,
            no_unused_parameters: options.no_unused_parameters,
            always_strict: options.always_strict.unwrap_or(options.strict || defaults.always_strict),
            resolve_json_module: options.resolve_json_module,
            check_js: options.check_js,
            allow_js: false,
            no_resolve: options.no_resolve,
            isolated_declarations: false,
            emit_declarations: false,
            no_unchecked_side_effect_imports: options.no_unchecked_side_effect_imports,
            no_implicit_override: options.no_implicit_override,
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            jsx_mode: tsz_common::checker_options::JsxMode::None,
            module_explicitly_set: options.module.is_some(),
            suppress_excess_property_errors: false,
            suppress_implicit_any_index_errors: false,
            no_implicit_use_strict: false,
            allow_importing_ts_extensions: false,
            rewrite_relative_import_extensions: false,
            implied_classic_resolution: false,
            jsx_import_source: String::new(),
            verbatim_module_syntax: false,
            ignore_deprecations: false,
            allow_umd_global_access: false,
            preserve_const_enums: false,
            strict_builtin_iterator_return: options.strict || defaults.strict_builtin_iterator_return,
            erasable_syntax_only: false,
        }
    }
}
