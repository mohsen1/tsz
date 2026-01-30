//! tsz-server: TypeScript language server compatible with tsserver
//!
//! This binary provides both:
//! 1. A tsserver-compatible stdin/stdout protocol (Content-Length framed JSON)
//! 2. A legacy JSON-per-line protocol for fast conformance testing
//!
//! tsserver Protocol (default):
//! - Input: Content-Length framed JSON on stdin
//! - Output: Content-Length framed JSON on stdout
//!
//! Legacy Protocol (--protocol legacy):
//! - Input: JSON objects on stdin (one per line)
//! - Output: JSON objects on stdout (one per line)
//!
//! tsserver-compatible CLI flags:
//!   --syntaxOnly, --useSingleInferredProject, --useInferredProjectPerProjectRoot,
//!   --suppressDiagnosticEvents, --cancellationPipeName, --serverMode, --locale,
//!   --logVerbosity, --logFile, --globalPlugins, --pluginProbeLocations, etc.
//!
//! Environment variables (tsserver-compatible):
//!   TSS_LOG     - Configure logging (e.g., "-level verbose -file /tmp/tsserver.log")
//!   TSS_DEBUG   - Enable debug mode on specified port
//!   TSS_DEBUG_BRK - Enable debug mode with break on startup
//!
//! Legacy usage:
//! ```bash
//! echo '{"type":"check","id":1,"files":{"main.ts":"const x: string = 1;"}}' | tsz-server --protocol legacy
//! ```

use anyhow::{Context, Result};
use clap::Parser;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read as IoRead, Write};
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

// =============================================================================
// CLI Arguments (tsserver-compatible)
// =============================================================================

/// tsz-server: TypeScript language server (tsserver-compatible)
#[derive(Parser, Debug)]
#[command(
    name = "tsz-server",
    version,
    about = "TypeScript language server - tsserver compatible"
)]
struct ServerArgs {
    /// Enable syntax-only mode (no semantic analysis).
    /// Legacy flag; prefer --serverMode partialSemantic.
    #[arg(long = "syntaxOnly", alias = "syntax-only")]
    syntax_only: bool,

    /// Consolidate all open files without a tsconfig into a single inferred project.
    #[arg(
        long = "useSingleInferredProject",
        alias = "use-single-inferred-project"
    )]
    use_single_inferred_project: bool,

    /// Create a separate inferred project for each distinct project root directory.
    #[arg(
        long = "useInferredProjectPerProjectRoot",
        alias = "use-inferred-project-per-project-root"
    )]
    use_inferred_project_per_project_root: bool,

    /// Disable automatic diagnostic discovery events.
    #[arg(
        long = "suppressDiagnosticEvents",
        alias = "suppress-diagnostic-events"
    )]
    suppress_diagnostic_events: bool,

    /// Opt out of starting getErr when projectsUpdatedInBackground fires.
    #[arg(
        long = "noGetErrOnBackgroundUpdate",
        alias = "no-get-err-on-background-update"
    )]
    no_get_err_on_background_update: bool,

    /// Allow loading language service plugins from local project node_modules.
    #[arg(
        long = "allowLocalPluginLoads",
        alias = "allow-local-plugin-loads"
    )]
    allow_local_plugin_loads: bool,

    /// Enable integration with the editor's file watcher.
    #[arg(long = "canUseWatchEvents", alias = "can-use-watch-events")]
    can_use_watch_events: bool,

    /// Disable Automatic Type Acquisition (ATA) for JavaScript projects.
    #[arg(
        long = "disableAutomaticTypingAcquisition",
        alias = "disable-automatic-typing-acquisition"
    )]
    disable_automatic_typing_acquisition: bool,

    /// Enable telemetry events.
    #[arg(long = "enableTelemetry", alias = "enable-telemetry")]
    enable_telemetry: bool,

    /// Validate the default npm binary location on startup.
    #[arg(
        long = "validateDefaultNpmLocation",
        alias = "validate-default-npm-location"
    )]
    validate_default_npm_location: bool,

    /// Named pipe for request cancellation semaphore.
    /// If name ends with '*', actual pipe name is <name_without_*><requestId>.
    #[arg(
        long = "cancellationPipeName",
        alias = "cancellation-pipe-name"
    )]
    cancellation_pipe_name: Option<String>,

    /// Server operational mode: 'semantic' (default), 'partialSemantic', or 'syntactic'.
    #[arg(long = "serverMode", alias = "server-mode")]
    server_mode: Option<String>,

    /// TCP port for delivering events (if not specified, events go to stdout).
    #[arg(long = "eventPort", alias = "event-port")]
    event_port: Option<u16>,

    /// Language for error messages (e.g., en, ja, de).
    #[arg(long)]
    locale: Option<String>,

    /// Global TypeScript language service plugins (comma-separated).
    #[arg(
        long = "globalPlugins",
        alias = "global-plugins",
        value_delimiter = ','
    )]
    global_plugins: Option<Vec<String>>,

    /// Directories to search for plugin modules (comma-separated).
    #[arg(
        long = "pluginProbeLocations",
        alias = "plugin-probe-locations",
        value_delimiter = ','
    )]
    plugin_probe_locations: Option<Vec<String>>,

    /// Log verbosity level: off, terse, normal, requestTime, verbose.
    #[arg(long = "logVerbosity", alias = "log-verbosity")]
    log_verbosity: Option<String>,

    /// File path for server log output.
    #[arg(long = "logFile", alias = "log-file")]
    log_file: Option<PathBuf>,

    /// Directory for trace output files.
    #[arg(long = "traceDirectory", alias = "trace-directory")]
    trace_directory: Option<PathBuf>,

    /// Override the default npm binary location (for ATA).
    #[arg(long = "npmLocation", alias = "npm-location")]
    npm_location: Option<PathBuf>,

    /// Enable project-wide IntelliSense in web context.
    #[arg(
        long = "enableProjectWideIntelliSenseOnWeb",
        alias = "enable-project-wide-intellisense-on-web"
    )]
    enable_project_wide_intellisense_on_web: bool,

    /// Use Node.js IPC channel instead of stdin/stdout.
    #[arg(long = "useNodeIpc", alias = "use-node-ipc")]
    use_node_ipc: bool,

    // ==================== tsz-specific options ====================
    /// Protocol mode: 'tsserver' (Content-Length framed, default) or 'legacy' (JSON per line).
    #[arg(long, default_value = "tsserver")]
    protocol: Protocol,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
enum Protocol {
    /// tsserver-compatible Content-Length framed JSON protocol.
    Tsserver,
    /// Legacy JSON-per-line protocol for conformance testing.
    Legacy,
}

// =============================================================================
// tsserver Protocol Types
// =============================================================================

/// tsserver protocol message (incoming request)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TsServerRequest {
    seq: u64,
    #[serde(rename = "type")]
    msg_type: String,
    command: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

/// tsserver protocol response (outgoing)
#[derive(Debug, Serialize)]
struct TsServerResponse {
    seq: u64,
    #[serde(rename = "type")]
    msg_type: String,
    command: String,
    request_seq: u64,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<serde_json::Value>,
}

/// tsserver protocol event (outgoing, unsolicited)
#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct TsServerEvent {
    seq: u64,
    #[serde(rename = "type")]
    msg_type: String,
    event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<serde_json::Value>,
}

// =============================================================================
// Legacy Protocol Types (for conformance testing)
// =============================================================================

/// Legacy request from client
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum LegacyRequest {
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

/// Full compiler options for a check request (expanded for tsc compatibility)
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct CheckOptions {
    // === Strict Type Checking ===
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    strict_null_checks: Option<bool>,
    #[serde(default)]
    strict_function_types: Option<bool>,
    #[serde(default)]
    strict_bind_call_apply: Option<bool>,
    #[serde(default)]
    strict_property_initialization: Option<bool>,
    #[serde(default)]
    strict_builtin_iterator_return: Option<bool>,
    #[serde(default)]
    no_implicit_any: Option<bool>,
    #[serde(default)]
    no_implicit_this: Option<bool>,
    #[serde(default)]
    no_implicit_returns: bool,
    #[serde(default)]
    use_unknown_in_catch_variables: Option<bool>,
    #[serde(default)]
    always_strict: Option<bool>,

    // === Additional Type Checks ===
    #[serde(default)]
    no_unused_locals: bool,
    #[serde(default)]
    no_unused_parameters: bool,
    #[serde(default)]
    exact_optional_property_types: bool,
    #[serde(default)]
    no_fallthrough_cases_in_switch: bool,
    #[serde(default)]
    no_unchecked_indexed_access: bool,
    #[serde(default)]
    no_implicit_override: bool,
    #[serde(default)]
    no_property_access_from_index_signature: bool,
    #[serde(default)]
    no_unchecked_side_effect_imports: bool,
    #[serde(default)]
    allow_unreachable_code: Option<bool>,
    #[serde(default)]
    allow_unused_labels: Option<bool>,

    // === Module Interop ===
    #[serde(default)]
    es_module_interop: bool,
    #[serde(default)]
    allow_synthetic_default_imports: Option<bool>,
    #[serde(default)]
    isolated_modules: bool,
    #[serde(default)]
    isolated_declarations: bool,
    #[serde(default)]
    verbatim_module_syntax: bool,
    #[serde(default)]
    erasable_syntax_only: bool,

    // === Language & Environment ===
    #[serde(default)]
    no_lib: bool,
    #[serde(default)]
    lib: Option<Vec<String>>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    module: Option<String>,
    #[serde(default)]
    module_resolution: Option<String>,
    #[serde(default)]
    module_detection: Option<String>,
    #[serde(default)]
    jsx: Option<String>,
    #[serde(default)]
    jsx_factory: Option<String>,
    #[serde(default)]
    jsx_fragment_factory: Option<String>,
    #[serde(default)]
    jsx_import_source: Option<String>,
    #[serde(default)]
    react_namespace: Option<String>,
    #[serde(default)]
    experimental_decorators: bool,
    #[serde(default)]
    emit_decorator_metadata: bool,
    #[serde(default)]
    use_define_for_class_fields: Option<bool>,

    // === Emit ===
    #[serde(default)]
    no_emit: bool,
    #[serde(default)]
    no_emit_on_error: bool,
    #[serde(default)]
    declaration: bool,
    #[serde(default)]
    declaration_map: bool,
    #[serde(default)]
    emit_declaration_only: bool,
    #[serde(default)]
    source_map: bool,
    #[serde(default)]
    inline_source_map: bool,
    #[serde(default)]
    inline_sources: bool,
    #[serde(default)]
    out_dir: Option<String>,
    #[serde(default)]
    out_file: Option<String>,
    #[serde(default)]
    root_dir: Option<String>,
    #[serde(default)]
    remove_comments: bool,
    #[serde(default)]
    no_emit_helpers: bool,
    #[serde(default)]
    import_helpers: bool,
    #[serde(default)]
    downlevel_iteration: bool,
    #[serde(default)]
    preserve_const_enums: bool,
    #[serde(default)]
    strip_internal: bool,
    #[serde(default)]
    emit_bom: bool,
    #[serde(default)]
    new_line: Option<String>,

    // === JavaScript Support ===
    #[serde(default)]
    allow_js: bool,
    #[serde(default)]
    check_js: bool,
    #[serde(default)]
    max_node_module_js_depth: Option<u32>,

    // === Module Resolution ===
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    type_roots: Option<Vec<String>>,
    #[serde(default)]
    types: Option<Vec<String>>,
    #[serde(default)]
    root_dirs: Option<Vec<String>>,
    #[serde(default)]
    resolve_json_module: bool,
    #[serde(default)]
    resolve_package_json_exports: Option<bool>,
    #[serde(default)]
    resolve_package_json_imports: Option<bool>,
    #[serde(default)]
    module_suffixes: Option<Vec<String>>,
    #[serde(default)]
    allow_arbitrary_extensions: bool,
    #[serde(default)]
    allow_importing_ts_extensions: bool,
    #[serde(default)]
    rewrite_relative_import_extensions: bool,
    #[serde(default)]
    custom_conditions: Option<Vec<String>>,
    #[serde(default)]
    no_resolve: bool,
    #[serde(default)]
    allow_umd_global_access: bool,
    #[serde(default)]
    preserve_symlinks: bool,

    // === Completeness ===
    #[serde(default)]
    skip_lib_check: bool,
    #[serde(default)]
    skip_default_lib_check: bool,
    #[serde(default)]
    no_check: bool,

    // === Projects ===
    #[serde(default)]
    composite: bool,
    #[serde(default)]
    incremental: bool,

    // === Interop ===
    #[serde(default)]
    force_consistent_casing_in_file_names: Option<bool>,

    // === Diagnostics ===
    #[serde(default)]
    no_error_truncation: bool,
}

/// Legacy response to client
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum LegacyResponse {
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

// =============================================================================
// Logging Configuration (TSS_LOG environment variable)
// =============================================================================

struct LogConfig {
    level: LogLevel,
    file: Option<PathBuf>,
    trace_to_console: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogLevel {
    Off,
    Terse,
    Normal,
    RequestTime,
    Verbose,
}

impl LogConfig {
    fn from_env_and_args(args: &ServerArgs) -> Self {
        let mut config = LogConfig {
            level: LogLevel::Off,
            file: None,
            trace_to_console: false,
        };

        // Parse TSS_LOG environment variable
        // Format: -level <level> -traceToConsole <bool> -logToFile <bool> -file <path>
        if let Ok(tss_log) = std::env::var("TSS_LOG") {
            let parts: Vec<&str> = tss_log.split_whitespace().collect();
            let mut i = 0;
            while i < parts.len() {
                match parts[i] {
                    "-level" if i + 1 < parts.len() => {
                        config.level = match parts[i + 1] {
                            "terse" => LogLevel::Terse,
                            "normal" => LogLevel::Normal,
                            "requestTime" => LogLevel::RequestTime,
                            "verbose" => LogLevel::Verbose,
                            _ => LogLevel::Off,
                        };
                        i += 2;
                    }
                    "-file" if i + 1 < parts.len() => {
                        config.file = Some(PathBuf::from(parts[i + 1]));
                        i += 2;
                    }
                    "-traceToConsole" if i + 1 < parts.len() => {
                        config.trace_to_console = parts[i + 1] == "true";
                        i += 2;
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
        }

        // CLI args override TSS_LOG
        if let Some(ref verbosity) = args.log_verbosity {
            config.level = match verbosity.as_str() {
                "off" => LogLevel::Off,
                "terse" => LogLevel::Terse,
                "normal" => LogLevel::Normal,
                "requestTime" => LogLevel::RequestTime,
                "verbose" => LogLevel::Verbose,
                _ => LogLevel::Off,
            };
        }

        if let Some(ref log_file) = args.log_file {
            config.file = Some(log_file.clone());
        }

        config
    }
}

// =============================================================================
// Server State
// =============================================================================

struct Server {
    /// Directory containing lib.*.d.ts files (TypeScript/src/lib)
    lib_dir: PathBuf,
    /// Fallback directory for tests (TypeScript/tests/lib)
    tests_lib_dir: PathBuf,
    /// Cache of parsed+bound lib files AND their dependencies (references)
    lib_cache: FxHashMap<String, (Arc<LibFile>, Vec<String>)>,
    /// Number of checks completed
    checks_completed: u64,
    /// Response sequence counter (for tsserver protocol)
    response_seq: u64,
    /// Open files (for tsserver protocol)
    open_files: HashMap<String, String>,
    /// Server mode
    _server_mode: ServerMode,
    /// Log configuration
    _log_config: LogConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerMode {
    Semantic,
    PartialSemantic,
    Syntactic,
}

impl Server {
    fn new(args: &ServerArgs) -> Result<Self> {
        let lib_dir = Self::find_lib_dir()?;
        let tests_lib_dir = PathBuf::from("TypeScript/tests/lib");
        eprintln!("Using lib directory: {}", lib_dir.display());

        let server_mode = if args.syntax_only {
            ServerMode::Syntactic
        } else {
            match args.server_mode.as_deref() {
                Some("partialSemantic") => ServerMode::PartialSemantic,
                Some("syntactic") => ServerMode::Syntactic,
                _ => ServerMode::Semantic,
            }
        };

        let log_config = LogConfig::from_env_and_args(args);

        // Log TSS_DEBUG/TSS_DEBUG_BRK presence
        if let Ok(port) = std::env::var("TSS_DEBUG") {
            eprintln!("TSS_DEBUG detected: port {}", port);
        }
        if let Ok(port) = std::env::var("TSS_DEBUG_BRK") {
            eprintln!("TSS_DEBUG_BRK detected: port {} (break on startup)", port);
        }

        if log_config.level != LogLevel::Off {
            if let Some(ref file) = log_config.file {
                eprintln!("Log file: {}", file.display());
            }
            eprintln!("Log level: {:?}", log_config.level);
        }

        Ok(Self {
            lib_dir,
            tests_lib_dir,
            lib_cache: FxHashMap::default(),
            checks_completed: 0,
            response_seq: 0,
            open_files: HashMap::new(),
            _server_mode: server_mode,
            _log_config: log_config,
        })
    }

    fn next_seq(&mut self) -> u64 {
        self.response_seq += 1;
        self.response_seq
    }

    fn find_lib_dir() -> Result<PathBuf> {
        let cwd = std::env::current_dir().context("Failed to get CWD")?;

        // Allow override via environment variable
        if let Ok(dir) = std::env::var("TSZ_LIB_DIR") {
            let path = PathBuf::from(&dir);
            let path = if path.is_absolute() {
                path
            } else {
                cwd.join(&path)
            };
            if path.exists() {
                return Ok(path);
            }
        }

        let lib_path = cwd.join("TypeScript/src/lib");
        if lib_path.exists() {
            return Ok(lib_path);
        }

        let mut current = cwd.clone();
        for _ in 0..10 {
            let candidate = current.join("TypeScript/src/lib");
            if candidate.exists() {
                return Ok(candidate);
            }
            current = match current.parent() {
                Some(p) => p.to_path_buf(),
                None => break,
            };
        }

        anyhow::bail!(
            "TypeScript lib directory not found. \
             CWD: {}. \
             Checked: TypeScript/src/lib (relative to CWD), TSZ_LIB_DIR env var, \
             and walked up 10 directories looking for TypeScript/src/lib. \
             Run from project root or set TSZ_LIB_DIR to an absolute path.",
            cwd.display()
        )
    }

    // =========================================================================
    // tsserver Protocol Handling
    // =========================================================================

    fn handle_tsserver_request(&mut self, request: TsServerRequest) -> TsServerResponse {
        let seq = self.next_seq();
        match request.command.as_str() {
            "open" => self.handle_open(seq, &request),
            "close" => self.handle_close(seq, &request),
            "configure" => self.handle_configure(seq, &request),
            "quickinfo" => self.handle_quickinfo(seq, &request),
            "definition" | "typeDefinition" => self.handle_definition(seq, &request),
            "references" => self.handle_references(seq, &request),
            "completions" | "completionInfo" => self.handle_completions(seq, &request),
            "completionEntryDetails" => self.handle_completion_details(seq, &request),
            "signatureHelp" => self.handle_signature_help(seq, &request),
            "semanticDiagnosticsSync" => self.handle_semantic_diagnostics_sync(seq, &request),
            "syntacticDiagnosticsSync" => self.handle_syntactic_diagnostics_sync(seq, &request),
            "suggestionDiagnosticsSync" => self.handle_suggestion_diagnostics_sync(seq, &request),
            "geterr" => self.handle_geterr(seq, &request),
            "geterrForProject" => self.handle_geterr_for_project(seq, &request),
            "navtree" | "navbar" => self.handle_navtree(seq, &request),
            "documentHighlights" => self.handle_document_highlights(seq, &request),
            "rename" => self.handle_rename(seq, &request),
            "getCodeFixes" => self.handle_get_code_fixes(seq, &request),
            "getCombinedCodeFix" => self.handle_get_combined_code_fix(seq, &request),
            "getApplicableRefactors" => self.handle_get_applicable_refactors(seq, &request),
            "getEditsForRefactor" => self.handle_get_edits_for_refactor(seq, &request),
            "organizeImports" => self.handle_organize_imports(seq, &request),
            "getEditsForFileRename" => self.handle_get_edits_for_file_rename(seq, &request),
            "format" => self.handle_format(seq, &request),
            "formatonkey" => self.handle_format_on_key(seq, &request),
            "projectInfo" => self.handle_project_info(seq, &request),
            "compilerOptionsForInferredProjects" => {
                self.handle_compiler_options_for_inferred(seq, &request)
            }
            "openExternalProject" | "closeExternalProject" => {
                self.handle_external_project(seq, &request)
            }
            "updateOpen" => self.handle_update_open(seq, &request),
            "inlayHints" => self.handle_inlay_hints(seq, &request),
            "selectionRange" => self.handle_selection_range(seq, &request),
            "linkedEditingRange" => self.handle_linked_editing_range(seq, &request),
            "prepareCallHierarchy" => self.handle_prepare_call_hierarchy(seq, &request),
            "provideCallHierarchyIncomingCalls" | "provideCallHierarchyOutgoingCalls" => {
                self.handle_call_hierarchy(seq, &request)
            }
            "mapCode" => self.handle_map_code(seq, &request),
            "fileReferences" => self.handle_file_references(seq, &request),
            "implementation" => self.handle_implementation(seq, &request),
            "getOutliningSpans" => self.handle_outlining_spans(seq, &request),
            "exit" => TsServerResponse {
                seq,
                msg_type: "response".to_string(),
                command: request.command.clone(),
                request_seq: request.seq,
                success: true,
                message: None,
                body: None,
            },
            _ => TsServerResponse {
                seq,
                msg_type: "response".to_string(),
                command: request.command.clone(),
                request_seq: request.seq,
                success: false,
                message: Some(format!("Unrecognized command: {}", request.command)),
                body: None,
            },
        }
    }

    fn handle_open(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let file = request.arguments.get("file").and_then(|v| v.as_str());
        let content = request
            .arguments
            .get("fileContent")
            .and_then(|v| v.as_str());

        if let Some(file_path) = file {
            let text = if let Some(c) = content {
                c.to_string()
            } else {
                std::fs::read_to_string(file_path).unwrap_or_default()
            };
            self.open_files.insert(file_path.to_string(), text);
        }

        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "open".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: None,
        }
    }

    fn handle_close(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let file = request.arguments.get("file").and_then(|v| v.as_str());
        if let Some(file_path) = file {
            self.open_files.remove(file_path);
        }

        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "close".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: None,
        }
    }

    fn handle_configure(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        // Accept configuration but most options are not yet wired
        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "configure".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: None,
        }
    }

    fn handle_semantic_diagnostics_sync(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let file = request.arguments.get("file").and_then(|v| v.as_str());
        let diagnostics = if let Some(file_path) = file {
            if let Some(content) = self.open_files.get(file_path).cloned() {
                let files = HashMap::from([(file_path.to_string(), content)]);
                let options = CheckOptions::default();
                match self.run_check(files, options) {
                    Ok(codes) => codes
                        .iter()
                        .map(|code| {
                            serde_json::json!({
                                "start": {"line": 1, "offset": 1},
                                "end": {"line": 1, "offset": 1},
                                "text": format!("error TS{}: type error", code),
                                "code": code,
                                "category": "error"
                            })
                        })
                        .collect(),
                    Err(_) => vec![],
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "semanticDiagnosticsSync".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: Some(serde_json::json!(diagnostics)),
        }
    }

    fn handle_syntactic_diagnostics_sync(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let file = request.arguments.get("file").and_then(|v| v.as_str());
        let diagnostics: Vec<serde_json::Value> = if let Some(file_path) = file {
            if let Some(content) = self.open_files.get(file_path).cloned() {
                let mut parser =
                    ParserState::new(file_path.to_string(), content);
                let _root = parser.parse_source_file();
                parser
                    .get_diagnostics()
                    .iter()
                    .map(|d| {
                        serde_json::json!({
                            "start": {"line": 1, "offset": 1},
                            "end": {"line": 1, "offset": 1},
                            "text": format!("error TS{}: parse error", d.code),
                            "code": d.code,
                            "category": "error"
                        })
                    })
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "syntacticDiagnosticsSync".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: Some(serde_json::json!(diagnostics)),
        }
    }

    // Stub handlers for protocol commands - return success with empty/minimal responses
    fn stub_response(
        &self,
        seq: u64,
        request: &TsServerRequest,
        body: Option<serde_json::Value>,
    ) -> TsServerResponse {
        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: request.command.clone(),
            request_seq: request.seq,
            success: true,
            message: None,
            body,
        }
    }

    fn handle_quickinfo(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!({})))
    }

    fn handle_definition(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_references(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"refs": [], "symbolName": ""})),
        )
    }

    fn handle_completions(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"isGlobalCompletion": false, "isMemberCompletion": false, "isNewIdentifierLocation": false, "entries": []})),
        )
    }

    fn handle_completion_details(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_signature_help(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, None)
    }

    fn handle_suggestion_diagnostics_sync(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_geterr(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        // geterr is async in tsserver - it fires diagnostic events
        // For now, just acknowledge the request
        self.stub_response(seq, request, None)
    }

    fn handle_geterr_for_project(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, None)
    }

    fn handle_navtree(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"text": "<module>", "kind": "module", "childItems": []})),
        )
    }

    fn handle_document_highlights(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_rename(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"info": {"canRename": false, "localizedErrorMessage": "Not yet implemented"}, "locs": []})),
        )
    }

    fn handle_get_code_fixes(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_get_combined_code_fix(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"changes": [], "commands": []})),
        )
    }

    fn handle_get_applicable_refactors(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_get_edits_for_refactor(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"edits": []})),
        )
    }

    fn handle_organize_imports(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_get_edits_for_file_rename(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_format(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_format_on_key(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_project_info(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"configFileName": "", "fileNames": []})),
        )
    }

    fn handle_compiler_options_for_inferred(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, None)
    }

    fn handle_external_project(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, None)
    }

    fn handle_update_open(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        // Process opened, changed, and closed files
        if let Some(opened) = request.arguments.get("openFiles").and_then(|v| v.as_array()) {
            for entry in opened {
                if let (Some(file), Some(content)) = (
                    entry.get("file").and_then(|v| v.as_str()),
                    entry.get("fileContent").and_then(|v| v.as_str()),
                ) {
                    self.open_files
                        .insert(file.to_string(), content.to_string());
                }
            }
        }
        if let Some(closed) = request
            .arguments
            .get("closedFiles")
            .and_then(|v| v.as_array())
        {
            for entry in closed {
                if let Some(file) = entry.as_str() {
                    self.open_files.remove(file);
                }
            }
        }

        self.stub_response(seq, request, None)
    }

    fn handle_inlay_hints(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_selection_range(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_linked_editing_range(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, None)
    }

    fn handle_prepare_call_hierarchy(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_call_hierarchy(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_map_code(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_file_references(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"refs": [], "symbolName": ""})),
        )
    }

    fn handle_implementation(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_outlining_spans(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    // =========================================================================
    // Legacy Protocol Handling
    // =========================================================================

    fn handle_legacy_request(&mut self, request: LegacyRequest) -> LegacyResponse {
        match request {
            LegacyRequest::Check { id, files, options } => self.handle_legacy_check(id, files, options),
            LegacyRequest::Status { id } => self.handle_legacy_status(id),
            LegacyRequest::Recycle { id } => self.handle_legacy_recycle(id),
            LegacyRequest::Shutdown { id } => LegacyResponse::Ok(OkResponse { id, ok: true }),
        }
    }

    fn handle_legacy_check(
        &mut self,
        id: u64,
        files: HashMap<String, String>,
        options: CheckOptions,
    ) -> LegacyResponse {
        let start = Instant::now();
        match self.run_check(files, options) {
            Ok(codes) => {
                self.checks_completed += 1;
                LegacyResponse::Check(CheckResponse {
                    id,
                    codes,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                })
            }
            Err(e) => LegacyResponse::Error(ErrorResponse {
                id,
                error: e.to_string(),
            }),
        }
    }

    fn handle_legacy_status(&self, id: u64) -> LegacyResponse {
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

        LegacyResponse::Status(StatusResponse {
            id,
            memory_mb,
            checks_completed: self.checks_completed,
            cached_libs: self.lib_cache.len(),
        })
    }

    fn handle_legacy_recycle(&mut self, id: u64) -> LegacyResponse {
        self.lib_cache.clear();
        self.checks_completed = 0;
        LegacyResponse::Ok(OkResponse { id, ok: true })
    }

    // =========================================================================
    // Core Type Checking (shared between protocols)
    // =========================================================================

    fn run_check(
        &mut self,
        files: HashMap<String, String>,
        options: CheckOptions,
    ) -> Result<Vec<i32>> {
        let lib_files = if options.no_lib {
            vec![]
        } else {
            self.load_libs(&options)?
        };

        let checker_options = self.build_checker_options(&options);
        let type_interner = TypeInterner::new();

        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: lib.arena.clone(),
                binder: lib.binder.clone(),
            })
            .collect();

        // PHASE 1: Parse all files
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

        // PHASE 2: Bind all files
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

        // PHASE 3: Build cross-file resolution context
        let all_arenas: Vec<Arc<NodeArena>> =
            bound_files.iter().map(|f| f.arena.clone()).collect();
        let all_binders: Vec<Arc<BinderState>> =
            bound_files.iter().map(|f| f.binder.clone()).collect();
        let user_file_contexts: Vec<LibContext> = bound_files
            .iter()
            .map(|f| LibContext {
                arena: f.arena.clone(),
                binder: f.binder.clone(),
            })
            .collect();

        let mut all_contexts = lib_contexts;
        all_contexts.extend(user_file_contexts);

        let file_names: Vec<String> = bound_files.iter().map(|f| f.name.clone()).collect();
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

        // PHASE 4: Type check all files
        let mut all_codes: Vec<i32> = Vec::new();
        for (file_idx, bound) in bound_files.iter().enumerate() {
            all_codes.extend(&bound.parse_errors);

            let mut checker = CheckerState::new(
                &bound.arena,
                &bound.binder,
                &type_interner,
                bound.name.clone(),
                checker_options.clone(),
            );

            if !all_contexts.is_empty() {
                checker.ctx.set_lib_contexts(all_contexts.clone());
            }

            checker.ctx.set_all_arenas(all_arenas.clone());
            checker.ctx.set_all_binders(all_binders.clone());
            checker
                .ctx
                .set_resolved_module_paths(resolved_module_paths.clone());
            checker.ctx.set_resolved_modules(resolved_modules.clone());
            checker.ctx.set_current_file_idx(file_idx);
            checker.check_source_file(bound.root);

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
        for lib_name in lib_names {
            self.load_lib_recursive(&lib_name, &mut result, &mut loaded)?;
        }
        Ok(result)
    }

    fn load_lib_recursive(
        &mut self,
        lib_name: &str,
        result: &mut Vec<Arc<LibFile>>,
        loaded: &mut rustc_hash::FxHashSet<String>,
    ) -> Result<()> {
        let normalized = lib_name.trim().to_lowercase();
        if loaded.contains(&normalized) {
            return Ok(());
        }
        loaded.insert(normalized.clone());

        if let Some((lib, references)) = self.lib_cache.get(&normalized) {
            let lib_clone = lib.clone();
            let refs = references.clone();
            for ref_lib in &refs {
                self.load_lib_recursive(ref_lib, result, loaded)?;
            }
            result.push(lib_clone);
            return Ok(());
        }

        let candidates = [
            self.lib_dir.join(format!("{}.d.ts", normalized)),
            self.lib_dir.join(format!("lib.{}.d.ts", normalized)),
            self.tests_lib_dir.join(format!("{}.d.ts", normalized)),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                let content = std::fs::read_to_string(candidate)
                    .with_context(|| format!("failed to read lib file: {}", candidate.display()))?;
                let references = Self::parse_lib_references(&content);
                for ref_lib in &references {
                    self.load_lib_recursive(ref_lib, result, loaded)?;
                }

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

                self.lib_cache
                    .insert(normalized, (lib.clone(), references.clone()));
                result.push(lib);
                return Ok(());
            }
        }

        Ok(())
    }

    fn parse_lib_references(content: &str) -> Vec<String> {
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

    fn determine_libs(&self, options: &CheckOptions) -> Vec<String> {
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
            target: checker_target,
            es_module_interop: options.es_module_interop,
            allow_synthetic_default_imports: options.allow_synthetic_default_imports.unwrap_or(
                options.es_module_interop,
            ),
        }
    }
}

// =============================================================================
// Protocol I/O
// =============================================================================

/// Read a Content-Length framed message from stdin (tsserver protocol)
fn read_content_length_message(reader: &mut BufReader<std::io::Stdin>) -> Result<Option<String>> {
    let mut header_line = String::new();
    let bytes_read = reader.read_line(&mut header_line)?;
    if bytes_read == 0 {
        return Ok(None); // EOF
    }

    let header = header_line.trim();
    if header.is_empty() {
        // Skip empty lines (can happen between messages)
        return read_content_length_message(reader);
    }

    // Parse Content-Length header
    let content_length = if let Some(len_str) = header.strip_prefix("Content-Length:") {
        len_str.trim().parse::<usize>().with_context(|| {
            format!("invalid Content-Length: {}", len_str.trim())
        })?
    } else {
        // Not a Content-Length header - try to parse as raw JSON (for compatibility)
        return Ok(Some(header.to_string()));
    };

    // Read the blank line separator
    let mut blank_line = String::new();
    reader.read_line(&mut blank_line)?;

    // Read the message body
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    String::from_utf8(body)
        .map(Some)
        .context("invalid UTF-8 in message body")
}

/// Write a Content-Length framed message to stdout (tsserver protocol)
fn write_content_length_message(stdout: &mut std::io::Stdout, message: &str) -> Result<()> {
    write!(stdout, "Content-Length: {}\r\n\r\n{}", message.len(), message)?;
    stdout.flush()?;
    Ok(())
}

// =============================================================================
// Main Entry Point
// =============================================================================

fn main() -> Result<()> {
    // Initialize tracing (stderr so it doesn't interfere with protocol)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = ServerArgs::parse();
    let mut server = Server::new(&args).context("failed to initialize server")?;

    eprintln!("tsz-server ready (protocol: {:?})", args.protocol);

    match args.protocol {
        Protocol::Tsserver => run_tsserver_protocol(&mut server)?,
        Protocol::Legacy => run_legacy_protocol(&mut server)?,
    }

    Ok(())
}

fn run_tsserver_protocol(server: &mut Server) -> Result<()> {
    let mut stdin = BufReader::new(std::io::stdin());
    let mut stdout = std::io::stdout();

    loop {
        let message = match read_content_length_message(&mut stdin)? {
            Some(msg) => msg,
            None => break, // EOF
        };

        if message.trim().is_empty() {
            continue;
        }

        let request: TsServerRequest = match serde_json::from_str(&message) {
            Ok(req) => req,
            Err(e) => {
                let error_response = TsServerResponse {
                    seq: server.next_seq(),
                    msg_type: "response".to_string(),
                    command: "unknown".to_string(),
                    request_seq: 0,
                    success: false,
                    message: Some(format!("invalid request: {}", e)),
                    body: None,
                };
                let json = serde_json::to_string(&error_response)?;
                write_content_length_message(&mut stdout, &json)?;
                continue;
            }
        };

        let is_exit = request.command == "exit";
        let response = server.handle_tsserver_request(request);
        let json = serde_json::to_string(&response)?;
        write_content_length_message(&mut stdout, &json)?;

        if is_exit {
            break;
        }
    }

    Ok(())
}

fn run_legacy_protocol(server: &mut Server) -> Result<()> {
    let stdin = BufReader::new(std::io::stdin());
    let mut stdout = std::io::stdout();

    for line in stdin.lines() {
        let line = line.context("failed to read from stdin")?;
        if line.trim().is_empty() {
            continue;
        }

        let request: LegacyRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = LegacyResponse::Error(ErrorResponse {
                    id: 0,
                    error: format!("invalid request: {}", e),
                });
                writeln!(stdout, "{}", serde_json::to_string(&error_response)?)?;
                stdout.flush()?;
                continue;
            }
        };

        let is_shutdown = matches!(request, LegacyRequest::Shutdown { .. });
        let response = server.handle_legacy_request(request);
        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;

        if is_shutdown {
            break;
        }
    }

    Ok(())
}
