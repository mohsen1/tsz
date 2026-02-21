//! Core type checking logic for tsz-server.
//!
//! Contains the check pipeline: semantic diagnostics, `run_check`, lib loading,
//! and checker option construction.

use super::{CheckOptions, Server, TsServerRequest, TsServerResponse};
use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz::binder::BinderState;
use tsz::checker::context::{CheckerOptions, LibContext};
use tsz::checker::diagnostics::DiagnosticCategory;
use tsz::checker::module_resolution::build_module_resolution_maps;
use tsz::checker::state::CheckerState;
use tsz::emitter::ScriptTarget;
use tsz::lib_loader::LibFile;
use tsz::parser::ParserState;
use tsz::parser::base::NodeIndex;
use tsz::parser::node::NodeArena;
use tsz_cli::config::{checker_target_from_emitter, default_lib_name_for_target};
use tsz_solver::QueryCache;
use tsz_solver::RelationCacheStats;
use tsz_solver::TypeInterner;

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
        let options = CheckOptions::default();

        // Use unified lib loading for proper cross-lib symbol resolution.
        // The unified binder has declaration_arenas tracking each declaration's source arena.
        let lib_files = match if options.no_lib {
            Ok(vec![])
        } else {
            self.load_libs_unified(&options)
        } {
            Ok(libs) => libs,
            Err(_) => return Vec::new(),
        };

        let checker_options = self.build_checker_options(&options);
        let type_interner = TypeInterner::new();

        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();

        let mut parser = ParserState::new(file_path.to_string(), content.to_string());
        let root = parser.parse_source_file();
        let parse_diagnostics = parser.get_diagnostics().to_vec();
        let arena = Arc::new(parser.into_arena());
        let mut binder = BinderState::new();
        binder.bind_source_file(&arena, root);
        let binder = Arc::new(binder);

        let all_arenas: Arc<Vec<Arc<NodeArena>>> = Arc::new(vec![std::sync::Arc::clone(&arena)]);
        let all_binders: Arc<Vec<Arc<BinderState>>> =
            Arc::new(vec![std::sync::Arc::clone(&binder)]);
        let user_file_contexts: Vec<LibContext> = vec![LibContext {
            arena: std::sync::Arc::clone(&arena),
            binder: std::sync::Arc::clone(&binder),
        }];

        let mut all_contexts = lib_contexts;
        all_contexts.extend(user_file_contexts);

        let file_names = vec![file_path.to_string()];
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
        let resolved_module_paths = Arc::new(resolved_module_paths);

        let query_cache = QueryCache::new(&type_interner);

        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &query_cache,
            file_path.to_string(),
            checker_options,
        );

        if !all_contexts.is_empty() {
            checker.ctx.set_lib_contexts(all_contexts);
        }
        // Set the count of actual lib files (not user files) for has_lib_loaded()
        checker.ctx.set_actual_lib_file_count(lib_files.len());

        checker.ctx.set_all_arenas(all_arenas);
        checker.ctx.set_all_binders(all_binders);
        checker.ctx.set_resolved_module_paths(resolved_module_paths);
        checker.ctx.set_resolved_modules(resolved_modules);
        checker.ctx.set_current_file_idx(0);
        checker.check_source_file(root);

        let mut diagnostics: Vec<tsz::checker::diagnostics::Diagnostic> = Vec::new();

        // Add parse diagnostics (only for Errors)
        if category == DiagnosticCategory::Error {
            for d in &parse_diagnostics {
                diagnostics.push(tsz::checker::diagnostics::Diagnostic::error(
                    file_path.to_string(),
                    d.start,
                    d.length,
                    d.message.clone(),
                    d.code,
                ));
            }
        }

        // Add checker diagnostics
        for diag in checker.ctx.diagnostics {
            if diag.category == category {
                diagnostics.push(diag);
            }
        }

        diagnostics
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
        // Use unified lib loading for proper cross-lib symbol resolution.
        // The unified binder has declaration_arenas tracking each declaration's source arena.
        let lib_files = if options.no_lib {
            vec![]
        } else {
            self.load_libs_unified(&options)?
        };

        let checker_options = self.build_checker_options(&options);
        let type_interner = TypeInterner::new();

        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();

        // PHASE 1 & 2: Parse and bind files in a single loop to reduce peak memory
        // This avoids holding all ASTs and source strings simultaneously
        struct BoundFile {
            name: String,
            arena: Arc<NodeArena>,
            binder: Arc<BinderState>,
            root: NodeIndex,
            parse_errors: Vec<i32>,
        }

        // CRITICAL: Fix SymbolId collisions by reserving lib SymbolIds in user binders
        //
        // Problem: Lib has symbols 0..N (Array, String, etc.) in its arena.
        // We copy these SymbolIds into user's file_locals via HashMap clone.
        // But user binder allocates new symbols from 0, eventually colliding with lib SymbolIds.
        //
        // Solution: Reserve SymbolIds 0..N in user's arena BEFORE binding.
        // This forces new allocations to start at N, preventing collisions.
        // Lib symbols are accessed via lib_binders fallback OR via reserved slots.
        let unified_lib_binder =
            (!lib_files.is_empty()).then(|| std::sync::Arc::clone(&lib_files[0].binder));

        // Count lib symbols to set base offset in user binders
        let lib_symbol_count = unified_lib_binder.as_ref().map_or(0, |b| b.symbols.len());

        let mut bound_files: Vec<BoundFile> = Vec::with_capacity(files.len());
        let mut binary_file_errors: Vec<(String, i32)> = Vec::new();

        // Use into_iter() to consume source strings immediately
        for (file_name, content) in files {
            // Skip non-TypeScript/JavaScript files (e.g. .json, .txt).
            // They may be present in multi-file tests for module resolution
            // fixtures but must not be parsed as TypeScript source.
            if !Self::is_checkable_file(&file_name) {
                continue;
            }

            // Check if content appears to be garbled binary (e.g., UTF-16 read as UTF-8)
            // If so, emit TS1490 "File appears to be binary." instead of parsing
            if super::content_appears_binary(&content) {
                binary_file_errors
                    .push((file_name.clone(), super::TS1490_FILE_APPEARS_TO_BE_BINARY));
                continue;
            }

            // Parse: content is moved into parser
            let mut parser = ParserState::new(file_name.clone(), content);
            let root_idx = parser.parse_source_file();
            let parse_errors: Vec<i32> = parser
                .get_diagnostics()
                .iter()
                .map(|d| d.code as i32)
                .collect();
            let arena = Arc::new(parser.into_arena());

            // Bind with SymbolId collision prevention:
            // 1. Set base offset for user arena to prevent allocation collisions
            // 2. Set lib_binders for lib symbol resolution (fallback)
            // 3. Do NOT copy file_locals - let lib symbols be resolved via lib_binders
            let mut binder = BinderState::new();
            if let Some(lib_binder) = unified_lib_binder.as_ref() {
                // Step 1: Set base offset so user symbols start AFTER lib symbols.
                // This ensures lookups for lib IDs (0..N) return None in the user arena,
                // triggering the fallback to lib_binders.
                binder.symbols = tsz::binder::SymbolArena::new_with_base(lib_symbol_count as u32);

                // Step 2: Set lib_binder for fallback resolution
                // Lib symbols will be resolved via get_global_type() which checks lib_binders
                binder.lib_binders.push(Arc::clone(lib_binder));
            }
            binder.bind_source_file(&arena, root_idx);

            bound_files.push(BoundFile {
                name: file_name,
                arena,
                binder: Arc::new(binder),
                root: root_idx,
                parse_errors,
            });
        }

        // PHASE 3: Build cross-file resolution context
        // Wrap in Arc to avoid expensive cloning in the loop below
        let all_arenas: Arc<Vec<Arc<NodeArena>>> = Arc::new(
            bound_files
                .iter()
                .map(|f| std::sync::Arc::clone(&f.arena))
                .collect(),
        );
        let all_binders: Arc<Vec<Arc<BinderState>>> = Arc::new(
            bound_files
                .iter()
                .map(|f| std::sync::Arc::clone(&f.binder))
                .collect(),
        );
        let user_file_contexts: Vec<LibContext> = bound_files
            .iter()
            .map(|f| LibContext {
                arena: std::sync::Arc::clone(&f.arena),
                binder: std::sync::Arc::clone(&f.binder),
            })
            .collect();

        let mut all_contexts = lib_contexts;
        all_contexts.extend(user_file_contexts);
        // Wrap in Arc to avoid cloning the entire vector for every file
        let all_contexts_arc: Arc<Vec<LibContext>> = Arc::new(all_contexts);

        let file_names: Vec<String> = bound_files.iter().map(|f| f.name.clone()).collect();
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
        // Wrap in Arc to avoid cloning HashMap/HashSet for every file
        let resolved_module_paths_arc: Arc<FxHashMap<(usize, String), usize>> =
            Arc::new(resolved_module_paths);
        let resolved_modules_arc: Arc<rustc_hash::FxHashSet<String>> = Arc::new(resolved_modules);

        // PHASE 4: Type check all files
        let query_cache = QueryCache::new(&type_interner);
        let mut all_codes: Vec<i32> = Vec::new();

        // Add TS1490 for binary files detected earlier
        for (_file_name, code) in binary_file_errors {
            all_codes.push(code);
        }
        for (file_idx, bound) in bound_files.iter().enumerate() {
            all_codes.extend(&bound.parse_errors);

            let mut checker = CheckerState::new(
                &bound.arena,
                &bound.binder,
                &query_cache,
                bound.name.clone(),
                checker_options.clone(),
            );

            // Clone Arc pointers (cheap) instead of entire data structures (expensive)
            if !all_contexts_arc.is_empty() {
                checker.ctx.set_lib_contexts((*all_contexts_arc).clone());
            }
            // Set the count of actual lib files (not user files) for has_lib_loaded()
            checker.ctx.set_actual_lib_file_count(lib_files.len());

            // NOTE: Keep report_unresolved_imports = false for conformance testing.
            // While cross-lib type merging is now implemented via unified binder,
            // enabling full error reporting causes extra TS2304/TS2307 errors for
            // multi-file tests that reference symbols from other files.
            // Single-file conformance mode doesn't have full module resolution context.

            checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
            checker.ctx.set_all_binders(Arc::clone(&all_binders));
            checker
                .ctx
                .set_resolved_module_paths(Arc::clone(&resolved_module_paths_arc));
            checker
                .ctx
                .set_resolved_modules((*resolved_modules_arc).clone());
            checker.ctx.set_current_file_idx(file_idx);
            checker.check_source_file(bound.root);

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

        let unified_lib = Arc::new(LibFile::new(
            "unified-libs".to_string(),
            unified_arena,
            Arc::new(unified_binder),
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
                        let quote_char = quote.unwrap();
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

        CheckerOptions {
            strict: options.strict,
            strict_null_checks: options.strict_null_checks.unwrap_or(options.strict),
            strict_function_types: options.strict_function_types.unwrap_or(options.strict),
            strict_bind_call_apply: options.strict_bind_call_apply.unwrap_or(options.strict),
            strict_property_initialization: options
                .strict_property_initialization
                .unwrap_or(options.strict),
            no_implicit_any: options.no_implicit_any.unwrap_or(options.strict),
            no_implicit_this: options.no_implicit_this.unwrap_or(options.strict),
            no_implicit_returns: options.no_implicit_returns,
            exact_optional_property_types: options.exact_optional_property_types,
            no_unchecked_indexed_access: options.no_unchecked_indexed_access,
            use_unknown_in_catch_variables: options
                .use_unknown_in_catch_variables
                .unwrap_or(options.strict),
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
                .unwrap_or(options.es_module_interop),
            allow_unreachable_code: options.allow_unreachable_code,
            no_property_access_from_index_signature: options
                .no_property_access_from_index_signature,
            sound_mode: false, // Sound mode not yet exposed in server protocol
            experimental_decorators: options.experimental_decorators,
            no_unused_locals: options.no_unused_locals,
            no_unused_parameters: options.no_unused_parameters,
            always_strict: options.always_strict.unwrap_or(options.strict),
            resolve_json_module: options.resolve_json_module,
            check_js: options.check_js,
            no_resolve: options.no_resolve,
            no_unchecked_side_effect_imports: options.no_unchecked_side_effect_imports,
            no_implicit_override: options.no_implicit_override,
            jsx_factory: "React.createElement".to_string(),
            jsx_fragment_factory: "React.Fragment".to_string(),
        }
    }
}
