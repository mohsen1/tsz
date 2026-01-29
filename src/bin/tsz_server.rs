//! tsz-server: Persistent type-checking server for fast conformance testing
//!
//! This binary provides a stdin/stdout JSON protocol (similar to tsserver) for
//! running type checks without process spawn overhead. Keeps TypeScript libs
//! loaded in memory for fast repeated checks.
//!
//! Protocol:
//! - Input: JSON objects on stdin (one per line)
//! - Output: JSON objects on stdout (one per line)
//!
//! Request types:
//! ```json
//! {"type": "check", "id": 1, "files": {"main.ts": "const x: string = 1;"}, "options": {"strict": true}}
//! {"type": "status", "id": 2}
//! {"type": "recycle", "id": 3}
//! {"type": "shutdown", "id": 4}
//! ```
//!
//! Usage:
//! ```bash
//! echo '{"type":"check","id":1,"files":{"main.ts":"const x: string = 1;"}}' | tsz-server
//! ```

use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use wasm::binder::BinderState;
use wasm::checker::context::{CheckerOptions, LibContext};
use wasm::checker::module_resolution::build_module_resolution_maps;
use wasm::checker::state::CheckerState;
use wasm::checker::types::diagnostics::DiagnosticCategory;
use wasm::cli::config::{checker_target_from_emitter, default_lib_name_for_target};
use wasm::emitter::ScriptTarget;
use wasm::lib_loader::LibFile;
use wasm::parser::ParserState;
use wasm::parser::base::NodeIndex;
use wasm::parser::node::NodeArena;
use wasm::solver::TypeInterner;

/// Request from client
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Request {
    /// Type check files and return error codes
    Check {
        id: u64,
        files: HashMap<String, String>,
        #[serde(default)]
        options: CheckOptions,
    },
    /// Get server status (memory usage, checks completed)
    Status { id: u64 },
    /// Clear caches and force memory cleanup
    Recycle { id: u64 },
    /// Graceful shutdown
    Shutdown { id: u64 },
}

/// Compiler options for a check request
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckOptions {
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    strict_null_checks: Option<bool>,
    #[serde(default)]
    strict_function_types: Option<bool>,
    #[serde(default)]
    no_implicit_any: Option<bool>,
    #[serde(default)]
    no_implicit_this: Option<bool>,
    #[serde(default)]
    no_implicit_returns: bool,
    #[serde(default)]
    no_lib: bool,
    #[serde(default)]
    lib: Option<Vec<String>>,
    #[serde(default)]
    target: Option<String>,
}

/// Response to client
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Response {
    Check(CheckResponse),
    Status(StatusResponse),
    Ok(OkResponse),
    Error(ErrorResponse),
}

#[derive(Debug, Serialize)]
struct CheckResponse {
    id: u64,
    codes: Vec<i32>,
    elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    id: u64,
    memory_mb: u64,
    checks_completed: u64,
    cached_libs: usize,
}

#[derive(Debug, Serialize)]
struct OkResponse {
    id: u64,
    ok: bool,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    id: u64,
    error: String,
}

/// Server state
struct Server {
    /// Directory containing lib.*.d.ts files (TypeScript/src/lib)
    lib_dir: PathBuf,
    /// Fallback directory for tests (TypeScript/tests/lib)
    tests_lib_dir: PathBuf,
    /// Cache of parsed+bound lib files AND their dependencies (references)
    /// Stores (lib, references) tuple to ensure dependencies are loaded on cache hit
    lib_cache: FxHashMap<String, (Arc<LibFile>, Vec<String>)>,
    /// Number of checks completed
    checks_completed: u64,
}

impl Server {
    fn new() -> Result<Self> {
        let lib_dir = Self::find_lib_dir()?;
        let tests_lib_dir = PathBuf::from("TypeScript/tests/lib");
        eprintln!("Using lib directory: {}", lib_dir.display());

        Ok(Self {
            lib_dir,
            tests_lib_dir,
            lib_cache: FxHashMap::default(),
            checks_completed: 0,
        })
    }

    fn find_lib_dir() -> Result<PathBuf> {
        // TypeScript lib directory is always at TypeScript/src/lib relative to project root.
        // This assumes tsz is run from the project root where TypeScript submodule exists.

        // Allow override via environment variable
        if let Ok(dir) = std::env::var("TSZ_LIB_DIR") {
            let path = PathBuf::from(dir);
            if path.exists() {
                return Ok(path);
            }
        }

        // The canonical path: TypeScript/src/lib
        let lib_path = PathBuf::from("TypeScript/src/lib");
        if lib_path.exists() {
            return Ok(lib_path);
        }

        anyhow::bail!(
            "TypeScript lib directory not found at TypeScript/src/lib. \
             Run from project root or set TSZ_LIB_DIR."
        )
    }

    fn handle_request(&mut self, request: Request) -> Response {
        match request {
            Request::Check { id, files, options } => self.handle_check(id, files, options),
            Request::Status { id } => self.handle_status(id),
            Request::Recycle { id } => self.handle_recycle(id),
            Request::Shutdown { id } => Response::Ok(OkResponse { id, ok: true }),
        }
    }

    fn handle_check(
        &mut self,
        id: u64,
        files: HashMap<String, String>,
        options: CheckOptions,
    ) -> Response {
        let start = Instant::now();

        match self.run_check(files, options) {
            Ok(codes) => {
                self.checks_completed += 1;
                Response::Check(CheckResponse {
                    id,
                    codes,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                })
            }
            Err(e) => Response::Error(ErrorResponse {
                id,
                error: e.to_string(),
            }),
        }
    }

    fn run_check(
        &mut self,
        files: HashMap<String, String>,
        options: CheckOptions,
    ) -> Result<Vec<i32>> {
        // Load lib files
        let lib_files = if options.no_lib {
            vec![]
        } else {
            self.load_libs(&options)?
        };

        // Build checker options
        let checker_options = self.build_checker_options(&options);

        // Create type interner (shared across all files)
        let type_interner = TypeInterner::new();

        // Build lib contexts for type resolution
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: lib.arena.clone(),
                binder: lib.binder.clone(),
            })
            .collect();

        // ========================================
        // PHASE 1: Parse all files
        // ========================================
        struct ParsedFile {
            name: String,
            arena: Arc<NodeArena>,
            root: NodeIndex,
            parse_errors: Vec<i32>,
        }

        let mut parsed_files: Vec<ParsedFile> = Vec::with_capacity(files.len());

        for (file_name, content) in &files {
            let mut parser = ParserState::new(file_name.clone(), content.clone());
            let root_idx = parser.parse_source_file();

            // Collect parse diagnostics
            let parse_errors: Vec<i32> = parser
                .get_diagnostics()
                .iter()
                .map(|d| d.code as i32)
                .collect();

            parsed_files.push(ParsedFile {
                name: file_name.clone(),
                arena: Arc::new(parser.into_arena()),
                root: root_idx,
                parse_errors,
            });
        }

        // ========================================
        // PHASE 2: Bind all files
        // ========================================
        struct BoundFile {
            name: String,
            arena: Arc<NodeArena>,
            binder: Arc<BinderState>,
            root: NodeIndex,
            parse_errors: Vec<i32>,
        }

        let mut bound_files: Vec<BoundFile> = Vec::with_capacity(parsed_files.len());

        for parsed in parsed_files {
            let mut binder = BinderState::new();
            binder.bind_source_file(&parsed.arena, parsed.root);

            bound_files.push(BoundFile {
                name: parsed.name,
                arena: parsed.arena,
                binder: Arc::new(binder),
                root: parsed.root,
                parse_errors: parsed.parse_errors,
            });
        }

        // ========================================
        // PHASE 3: Build cross-file resolution context
        // ========================================

        // Collect all arenas and binders for cross-file resolution
        let all_arenas: Vec<Arc<wasm::parser::node::NodeArena>> =
            bound_files.iter().map(|f| f.arena.clone()).collect();

        let all_binders: Vec<Arc<BinderState>> =
            bound_files.iter().map(|f| f.binder.clone()).collect();

        // Create LibContext for each user file (for global symbol resolution)
        let user_file_contexts: Vec<LibContext> = bound_files
            .iter()
            .map(|f| LibContext {
                arena: f.arena.clone(),
                binder: f.binder.clone(),
            })
            .collect();

        // Combine lib contexts with user file contexts
        let mut all_contexts = lib_contexts;
        all_contexts.extend(user_file_contexts);

        // Build module resolution maps using shared utility
        let file_names: Vec<String> = bound_files.iter().map(|f| f.name.clone()).collect();
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

        // ========================================
        // PHASE 4: Type check all files
        // ========================================
        let mut all_codes: Vec<i32> = Vec::new();

        for (file_idx, bound) in bound_files.iter().enumerate() {
            // Add parse errors
            all_codes.extend(&bound.parse_errors);

            // Create checker for this file
            let mut checker = CheckerState::new(
                &bound.arena,
                &bound.binder,
                &type_interner,
                bound.name.clone(),
                checker_options.clone(),
            );

            // Set all contexts for cross-file symbol resolution
            if !all_contexts.is_empty() {
                checker.ctx.set_lib_contexts(all_contexts.clone());
            }

            // Set multi-file resolution context
            checker.ctx.set_all_arenas(all_arenas.clone());
            checker.ctx.set_all_binders(all_binders.clone());
            checker
                .ctx
                .set_resolved_module_paths(resolved_module_paths.clone());
            checker.ctx.set_resolved_modules(resolved_modules.clone());
            checker.ctx.set_current_file_idx(file_idx);

            // Type check the file
            checker.check_source_file(bound.root);

            // Collect diagnostics
            for diag in &checker.ctx.diagnostics {
                if diag.category == DiagnosticCategory::Error {
                    all_codes.push(diag.code as i32);
                }
            }
        }

        Ok(all_codes)
    }

    fn load_libs(&mut self, options: &CheckOptions) -> Result<Vec<Arc<LibFile>>> {
        let lib_names = self.determine_libs(options);
        let mut result = Vec::new();
        let mut loaded = rustc_hash::FxHashSet::default();

        // Load libs with dependencies in order
        for lib_name in lib_names {
            self.load_lib_recursive(&lib_name, &mut result, &mut loaded)?;
        }

        Ok(result)
    }

    /// Load a lib and all its dependencies recursively.
    /// Dependencies are loaded first (depth-first) to ensure proper ordering.
    fn load_lib_recursive(
        &mut self,
        lib_name: &str,
        result: &mut Vec<Arc<LibFile>>,
        loaded: &mut rustc_hash::FxHashSet<String>,
    ) -> Result<()> {
        let normalized = lib_name.trim().to_lowercase();

        // Skip if already loaded
        if loaded.contains(&normalized) {
            return Ok(());
        }

        // Mark as loaded to prevent cycles
        loaded.insert(normalized.clone());

        // Check cache first
        if let Some((lib, references)) = self.lib_cache.get(&normalized) {
            // Clone lib and references to end the immutable borrow before recursive call
            let lib_clone = lib.clone();
            let refs = references.clone();

            // Load dependencies from the cached references list
            for ref_lib in &refs {
                self.load_lib_recursive(ref_lib, result, loaded)?;
            }
            result.push(lib_clone);
            return Ok(());
        }

        // Try to load from disk - check main lib dir first, then tests/lib fallback
        let candidates = [
            // Main lib dir (TypeScript/src/lib)
            self.lib_dir.join(format!("{}.d.ts", normalized)),
            self.lib_dir.join(format!("lib.{}.d.ts", normalized)),
            // Tests lib dir fallback (TypeScript/tests/lib) - for "lib" fallback
            self.tests_lib_dir.join(format!("{}.d.ts", normalized)),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                let content = std::fs::read_to_string(candidate)
                    .with_context(|| format!("failed to read lib file: {}", candidate.display()))?;

                // Parse /// <reference lib="..." /> directives BEFORE loading this lib
                // This ensures dependencies are loaded first
                let references = Self::parse_lib_references(&content);
                for ref_lib in &references {
                    self.load_lib_recursive(ref_lib, result, loaded)?;
                }

                // Now parse and bind this lib
                let file_name = candidate
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("lib.{}.d.ts", normalized));
                let mut parser = ParserState::new(file_name.clone(), content);
                let root_idx = parser.parse_source_file();

                let mut binder = BinderState::new();
                binder.bind_source_file(parser.get_arena(), root_idx);

                let lib = Arc::new(LibFile::new(
                    file_name,
                    Arc::new(parser.into_arena()),
                    Arc::new(binder),
                ));

                // Store both the lib and its references (dependencies) in the cache
                self.lib_cache
                    .insert(normalized, (lib.clone(), references.clone()));
                result.push(lib);
                return Ok(());
            }
        }

        Ok(())
    }

    /// Parse /// <reference lib="..." /> directives from lib file content.
    /// Uses simple string parsing instead of regex for performance.
    fn parse_lib_references(content: &str) -> Vec<String> {
        let mut refs = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            // Look for: /// <reference lib="..." />
            if !trimmed.starts_with("///") {
                continue;
            }
            if let Some(start) = trimmed.find("<reference") {
                let rest = &trimmed[start..];
                // Find lib=" or lib='
                if let Some(lib_start) = rest.find("lib=") {
                    let after_lib = &rest[lib_start + 4..];
                    // Get the quote character
                    let quote = after_lib.chars().next();
                    if quote == Some('"') || quote == Some('\'') {
                        let quote_char = quote.unwrap();
                        let value_start = 1; // skip the opening quote
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

    /// Determine which lib files to load based on compiler options.
    ///
    /// Uses the shared `default_lib_name_for_target` from cli::config to ensure
    /// CLI and server have identical lib/target resolution behavior.
    fn determine_libs(&self, options: &CheckOptions) -> Vec<String> {
        if options.no_lib {
            return vec![];
        }

        if let Some(ref libs) = options.lib {
            // Explicit lib option - use those libs exactly
            libs.iter().map(|s| s.trim().to_lowercase()).collect()
        } else {
            // No explicit lib - use the default lib for the target
            // This delegates to the shared core function to ensure CLI/server parity
            let target = Self::parse_target(&options.target);
            let default_lib = default_lib_name_for_target(target);
            vec![default_lib.to_string()]
        }
    }

    /// Parse a target string to ScriptTarget enum.
    /// Used by both determine_libs and build_checker_options.
    fn parse_target(target: &Option<String>) -> ScriptTarget {
        target
            .as_ref()
            .map(|t| match t.to_lowercase().as_str() {
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
            })
            .unwrap_or(ScriptTarget::ES5)
    }

    fn build_checker_options(&self, options: &CheckOptions) -> CheckerOptions {
        let emitter_target = Self::parse_target(&options.target);
        let checker_target = checker_target_from_emitter(emitter_target);

        CheckerOptions {
            strict: options.strict,
            strict_null_checks: options.strict_null_checks.unwrap_or(options.strict),
            strict_function_types: options.strict_function_types.unwrap_or(options.strict),
            strict_bind_call_apply: options.strict,
            strict_property_initialization: options.strict,
            no_implicit_any: options.no_implicit_any.unwrap_or(options.strict),
            no_implicit_this: options.no_implicit_this.unwrap_or(options.strict),
            no_implicit_returns: options.no_implicit_returns,
            exact_optional_property_types: false,
            no_unchecked_indexed_access: false,
            use_unknown_in_catch_variables: options.strict,
            isolated_modules: false,
            no_lib: options.no_lib,
            target: checker_target,
            es_module_interop: false,
            allow_synthetic_default_imports: false,
        }
    }

    fn handle_status(&self, id: u64) -> Response {
        // Get memory usage (approximate)
        let memory_mb = {
            #[cfg(target_os = "linux")]
            {
                std::fs::read_to_string("/proc/self/statm")
                    .ok()
                    .and_then(|s| s.split_whitespace().next()?.parse::<u64>().ok())
                    .map(|pages| pages * 4096 / 1024 / 1024)
                    .unwrap_or(0)
            }
            #[cfg(not(target_os = "linux"))]
            {
                0
            }
        };

        Response::Status(StatusResponse {
            id,
            memory_mb,
            checks_completed: self.checks_completed,
            cached_libs: self.lib_cache.len(),
        })
    }

    fn handle_recycle(&mut self, id: u64) -> Response {
        self.lib_cache.clear();
        self.checks_completed = 0;
        Response::Ok(OkResponse { id, ok: true })
    }
}

fn main() -> Result<()> {
    // Initialize tracing (stderr so it doesn't interfere with protocol)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut server = Server::new().context("failed to initialize server")?;

    // Signal readiness
    eprintln!("tsz-server ready");

    let stdin = BufReader::new(std::io::stdin());
    let mut stdout = std::io::stdout();

    for line in stdin.lines() {
        let line = line.context("failed to read from stdin")?;

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Parse request
        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = Response::Error(ErrorResponse {
                    id: 0,
                    error: format!("invalid request: {}", e),
                });
                writeln!(stdout, "{}", serde_json::to_string(&error_response)?)?;
                stdout.flush()?;
                continue;
            }
        };

        // Check for shutdown
        let is_shutdown = matches!(request, Request::Shutdown { .. });

        // Handle request
        let response = server.handle_request(request);

        // Write response
        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;

        // Exit on shutdown
        if is_shutdown {
            break;
        }
    }

    Ok(())
}
