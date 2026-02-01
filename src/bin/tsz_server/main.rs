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

mod handlers_editing;

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
use wasm::lsp::call_hierarchy::CallHierarchyProvider;
use wasm::lsp::completions::Completions;
use wasm::lsp::definition::GoToDefinition;
use wasm::lsp::document_symbols::DocumentSymbolProvider;
use wasm::lsp::folding::FoldingRangeProvider;
use wasm::lsp::highlighting::DocumentHighlightProvider;
use wasm::lsp::hover::HoverProvider;
use wasm::lsp::implementation::GoToImplementationProvider;
use wasm::lsp::inlay_hints::InlayHintsProvider;
use wasm::lsp::position::{LineMap, Position, Range};
use wasm::lsp::references::FindReferences;
use wasm::lsp::rename::RenameProvider;
use wasm::lsp::selection_range::SelectionRangeProvider;
use wasm::lsp::semantic_tokens::SemanticTokensProvider;
use wasm::lsp::signature_help::SignatureHelpProvider;
use wasm::parser::ParserState;
use wasm::parser::base::NodeIndex;
use wasm::parser::node::{NodeAccess, NodeArena};
use wasm::solver::TypeInterner;

// Diagnostic code for "File appears to be binary."
const TS1490_FILE_APPEARS_TO_BE_BINARY: i32 = 1490;

/// Check if content appears to be garbled binary (e.g., UTF-16 read as UTF-8).
///
/// When Node.js reads a UTF-16 file as UTF-8, it produces garbled output with:
/// - Replacement characters (U+FFFD)
/// - Many null bytes interspersed with ASCII
///
/// Returns true if content looks like corrupted binary that should emit TS1490.
fn content_appears_binary(content: &str) -> bool {
    if content.is_empty() {
        return false;
    }

    // Count problematic patterns in first 512 bytes (slice at character boundary)
    let max_bytes = content.len().min(512);
    // Find the character boundary closest to max_bytes
    let check_slice = if max_bytes >= content.len() {
        content
    } else {
        // Find the last character boundary at or before max_bytes
        let mut boundary = max_bytes;
        while !content.is_char_boundary(boundary) && boundary > 0 {
            boundary -= 1;
        }
        &content[..boundary]
    };

    // Check for replacement character (common when invalid UTF-8 sequences are read)
    let replacement_count = check_slice.matches('\u{FFFD}').count();
    if replacement_count >= 3 {
        return true;
    }

    // Check for null bytes (common in UTF-16 content read as UTF-8)
    // UTF-16 has null bytes between ASCII characters
    let null_count = check_slice.chars().filter(|&c| c == '\0').count();
    if null_count >= 4 {
        return true;
    }

    false
}

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
    #[arg(long = "allowLocalPluginLoads", alias = "allow-local-plugin-loads")]
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
    #[arg(long = "cancellationPipeName", alias = "cancellation-pipe-name")]
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
pub(crate) struct TsServerRequest {
    pub(crate) seq: u64,
    #[serde(rename = "type")]
    pub(crate) msg_type: String,
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) arguments: serde_json::Value,
}

/// tsserver protocol response (outgoing)
#[derive(Debug, Serialize)]
pub(crate) struct TsServerResponse {
    pub(crate) seq: u64,
    #[serde(rename = "type")]
    pub(crate) msg_type: String,
    pub(crate) command: String,
    pub(crate) request_seq: u64,
    pub(crate) success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) body: Option<serde_json::Value>,
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
#[allow(clippy::large_enum_variant)]
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

pub(crate) struct LogConfig {
    pub(crate) level: LogLevel,
    pub(crate) file: Option<PathBuf>,
    pub(crate) trace_to_console: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogLevel {
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

pub(crate) struct Server {
    /// Directory containing lib.*.d.ts files (TypeScript/src/lib)
    pub(crate) lib_dir: PathBuf,
    /// Fallback directory for tests (TypeScript/tests/lib)
    pub(crate) tests_lib_dir: PathBuf,
    /// Cache of parsed+bound lib files AND their dependencies (references)
    pub(crate) lib_cache: FxHashMap<String, (Arc<LibFile>, Vec<String>)>,
    /// Number of checks completed
    pub(crate) checks_completed: u64,
    /// Response sequence counter (for tsserver protocol)
    pub(crate) response_seq: u64,
    /// Open files (for tsserver protocol)
    pub(crate) open_files: HashMap<String, String>,
    /// Server mode
    pub(crate) _server_mode: ServerMode,
    /// Log configuration
    pub(crate) _log_config: LogConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerMode {
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

    // =========================================================================
    // Helper: Parse and Bind a File
    // =========================================================================

    /// Parse and bind a file from `open_files`, returning the arena, binder,
    /// root node index, and source text. Uses `into_arena()` to transfer
    /// the interner so that identifier resolution works correctly.
    fn parse_and_bind_file(
        &self,
        file_path: &str,
    ) -> Option<(NodeArena, BinderState, NodeIndex, String)> {
        let content = self.open_files.get(file_path)?.clone();
        let mut parser = ParserState::new(file_path.to_string(), content.clone());
        let root = parser.parse_source_file();
        let arena = parser.into_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(&arena, root);
        Some((arena, binder, root, content))
    }

    /// Extract file path, line, and offset from tsserver request arguments.
    /// Returns (file, line_1based, offset_1based).
    fn extract_file_position(args: &serde_json::Value) -> Option<(String, u32, u32)> {
        let file = args.get("file")?.as_str()?.to_string();
        let line = args.get("line")?.as_u64()? as u32;
        let offset = args.get("offset")?.as_u64()? as u32;
        Some((file, line, offset))
    }

    /// Convert tsserver 1-based line/offset to 0-based LSP Position.
    pub(crate) fn tsserver_to_lsp_position(line: u32, offset: u32) -> Position {
        Position::new(line.saturating_sub(1), offset.saturating_sub(1))
    }

    /// Convert LSP 0-based Position to tsserver 1-based {line, offset} JSON.
    fn lsp_to_tsserver_position(pos: &Position) -> serde_json::Value {
        serde_json::json!({
            "line": pos.line + 1,
            "offset": pos.character + 1
        })
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
            "change" => self.handle_change(seq, &request),
            "configure" => self.handle_configure(seq, &request),
            "quickinfo" => self.handle_quickinfo(seq, &request),
            "definition"
            | "typeDefinition"
            | "definition-full"
            | "typeDefinition-full"
            | "findSourceDefinition" => self.handle_definition(seq, &request),
            "definitionAndBoundSpan" | "definitionAndBoundSpan-full" => {
                self.handle_definition_and_bound_span(seq, &request)
            }
            "references" => self.handle_references(seq, &request),
            "references-full" => self.handle_references_full(seq, &request),
            "completions" | "completionInfo" => self.handle_completions(seq, &request),
            "completionEntryDetails" | "completionEntryDetails-full" => {
                self.handle_completion_details(seq, &request)
            }
            "signatureHelp" => self.handle_signature_help(seq, &request),
            "semanticDiagnosticsSync" => self.handle_semantic_diagnostics_sync(seq, &request),
            "syntacticDiagnosticsSync" => self.handle_syntactic_diagnostics_sync(seq, &request),
            "suggestionDiagnosticsSync" => self.handle_suggestion_diagnostics_sync(seq, &request),
            "geterr" => self.handle_geterr(seq, &request),
            "geterrForProject" => self.handle_geterr_for_project(seq, &request),
            "navtree" => self.handle_navtree(seq, &request),
            "navbar" => self.handle_navbar(seq, &request),
            "navto" | "navTo" | "navto-full" | "navTo-full" => self.handle_navto(seq, &request),
            "documentHighlights" => self.handle_document_highlights(seq, &request),
            "rename" | "rename-full" => self.handle_rename(seq, &request),
            "getCodeFixes" => self.handle_get_code_fixes(seq, &request),
            "getCombinedCodeFix" => self.handle_get_combined_code_fix(seq, &request),
            "getSupportedCodeFixes" => self.handle_get_supported_code_fixes(seq, &request),
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
            "encodedSemanticClassifications-full" => {
                self.handle_encoded_semantic_classifications_full(seq, &request)
            }
            "inlayHints" | "provideInlayHints" => self.handle_inlay_hints(seq, &request),
            "selectionRange" => self.handle_selection_range(seq, &request),
            "linkedEditingRange" => self.handle_linked_editing_range(seq, &request),
            "prepareCallHierarchy" => self.handle_prepare_call_hierarchy(seq, &request),
            "provideCallHierarchyIncomingCalls" | "provideCallHierarchyOutgoingCalls" => {
                self.handle_call_hierarchy(seq, &request)
            }
            "mapCode" => self.handle_map_code(seq, &request),
            "fileReferences" => self.handle_file_references(seq, &request),
            "implementation" | "implementation-full" => self.handle_implementation(seq, &request),
            "getOutliningSpans" => self.handle_outlining_spans(seq, &request),
            "brace" => self.stub_response(seq, &request, Some(serde_json::json!([]))),
            "emitOutput" | "emit-output" => self.stub_response(
                seq,
                &request,
                Some(serde_json::json!({"outputFiles": [], "emitSkipped": true})),
            ),
            "getMoveToRefactoringFileSuggestions" => self.stub_response(
                seq,
                &request,
                Some(serde_json::json!({"newFileName": "", "files": []})),
            ),
            "preparePasteEdits" => {
                self.stub_response(seq, &request, Some(serde_json::json!(false)))
            }
            "getPasteEdits" => self.stub_response(
                seq,
                &request,
                Some(serde_json::json!({"edits": [], "fixId": ""})),
            ),
            "configurePlugin" => self.stub_response(seq, &request, None),
            "breakpointStatement" => self.handle_breakpoint_statement(seq, &request),
            "jsxClosingTag" => self.handle_jsx_closing_tag(seq, &request),
            "braceCompletion" => self.handle_brace_completion(seq, &request),
            "getSpanOfEnclosingComment" => self.handle_span_of_enclosing_comment(seq, &request),
            "todoComments" => self.handle_todo_comments(seq, &request),
            "docCommentTemplate" => self.handle_doc_comment_template(seq, &request),
            "indentation" => self.handle_indentation(seq, &request),
            "toggleLineComment" | "toggleLineComment-full" => {
                self.handle_toggle_line_comment(seq, &request)
            }
            "toggleMultilineComment" | "toggleMultilineComment-full" => {
                self.handle_toggle_multiline_comment(seq, &request)
            }
            "commentSelection" | "commentSelection-full" => {
                self.handle_comment_selection(seq, &request)
            }
            "uncommentSelection" | "uncommentSelection-full" => {
                self.handle_uncomment_selection(seq, &request)
            }
            "getSmartSelectionRange" => self.handle_smart_selection_range(seq, &request),
            "getSyntacticClassifications" => self.handle_syntactic_classifications(seq, &request),
            "getSemanticClassifications" => self.handle_semantic_classifications(seq, &request),
            "getCompilerOptionsDiagnostics" => {
                self.handle_compiler_options_diagnostics(seq, &request)
            }
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

    fn handle_change(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let args = &request.arguments;
        let file = args.get("file").and_then(|v| v.as_str());

        if let Some(file_path) = file {
            let line = args.get("line").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let end_line = args
                .get("endLine")
                .and_then(|v| v.as_u64())
                .unwrap_or(line as u64) as u32;
            let end_offset = args
                .get("endOffset")
                .and_then(|v| v.as_u64())
                .unwrap_or(offset as u64) as u32;
            let insert_string = args
                .get("insertString")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if let Some(content) = self.open_files.get(file_path).cloned() {
                let new_content =
                    Self::apply_change(&content, line, offset, end_line, end_offset, insert_string);
                self.open_files.insert(file_path.to_string(), new_content);
            }
        }

        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "change".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: None,
        }
    }

    /// Apply a text change to file content.
    fn apply_change(
        content: &str,
        line: u32,
        offset: u32,
        end_line: u32,
        end_offset: u32,
        insert_string: &str,
    ) -> String {
        let start_byte = Self::line_offset_to_byte(content, line, offset);
        let end_byte = Self::line_offset_to_byte(content, end_line, end_offset);
        let mut result = String::with_capacity(
            content
                .len()
                .saturating_sub(end_byte.saturating_sub(start_byte))
                .saturating_add(insert_string.len()),
        );
        result.push_str(&content[..start_byte]);
        result.push_str(insert_string);
        result.push_str(&content[end_byte..]);
        result
    }

    /// Convert 1-based line/offset to a byte offset in the content string.
    fn line_offset_to_byte(content: &str, line: u32, offset: u32) -> usize {
        let target_line = (line as usize).saturating_sub(1);
        let target_col = (offset as usize).saturating_sub(1);
        let mut current_line = 0usize;
        let mut line_start = 0usize;
        if target_line > 0 {
            for (i, ch) in content.char_indices() {
                if ch == '\n' {
                    current_line += 1;
                    if current_line == target_line {
                        line_start = i + 1;
                        break;
                    }
                }
            }
            if current_line < target_line {
                return content.len();
            }
        }
        let mut byte_pos = line_start;
        for _ in 0..target_col {
            match content[byte_pos..].chars().next() {
                Some(c) if c != '\n' => byte_pos += c.len_utf8(),
                _ => break,
            }
        }
        byte_pos.min(content.len())
    }

    fn completion_kind_to_str(kind: wasm::lsp::completions::CompletionItemKind) -> &'static str {
        match kind {
            wasm::lsp::completions::CompletionItemKind::Variable => "var",
            wasm::lsp::completions::CompletionItemKind::Function => "function",
            wasm::lsp::completions::CompletionItemKind::Class => "class",
            wasm::lsp::completions::CompletionItemKind::Method => "method",
            wasm::lsp::completions::CompletionItemKind::Parameter => "parameter",
            wasm::lsp::completions::CompletionItemKind::Property => "property",
            wasm::lsp::completions::CompletionItemKind::Keyword => "keyword",
            wasm::lsp::completions::CompletionItemKind::Interface => "interface",
            wasm::lsp::completions::CompletionItemKind::Enum => "enum",
            wasm::lsp::completions::CompletionItemKind::TypeAlias => "type",
            wasm::lsp::completions::CompletionItemKind::Module => "module",
            wasm::lsp::completions::CompletionItemKind::TypeParameter => "type parameter",
            wasm::lsp::completions::CompletionItemKind::Constructor => "constructor",
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
        let include_line_position = request
            .arguments
            .get("includeLinePosition")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let diagnostics: Vec<serde_json::Value> = if let Some(file_path) = file {
            if let Some(content) = self.open_files.get(file_path).cloned() {
                let line_map = LineMap::build(&content);
                let full_diags = self.get_semantic_diagnostics_full(file_path, &content);
                full_diags
                    .iter()
                    .map(|diag| {
                        Self::format_diagnostic(
                            diag.start,
                            diag.length,
                            &diag.message_text,
                            diag.code,
                            &diag.category,
                            &line_map,
                            &content,
                            include_line_position,
                        )
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
        let include_line_position = request
            .arguments
            .get("includeLinePosition")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let diagnostics: Vec<serde_json::Value> = if let Some(file_path) = file {
            if let Some(content) = self.open_files.get(file_path).cloned() {
                let line_map = LineMap::build(&content);
                let mut parser = ParserState::new(file_path.to_string(), content.clone());
                let _root = parser.parse_source_file();
                parser
                    .get_diagnostics()
                    .iter()
                    .map(|d| {
                        Self::format_diagnostic(
                            d.start,
                            d.length,
                            &d.message,
                            d.code,
                            &DiagnosticCategory::Error,
                            &line_map,
                            &content,
                            include_line_position,
                        )
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

    /// Format a diagnostic for the tsserver protocol.
    ///
    /// When `include_line_position` is true (the SessionClient always sets this),
    /// the response includes 0-based `start`/`length` fields plus `startLocation`/
    /// `endLocation` with 1-based line/offset. When false, uses `start`/`end` as
    /// 1-based line/offset objects (the traditional tsserver format).
    fn format_diagnostic(
        start_offset: u32,
        length: u32,
        message: &str,
        code: u32,
        category: &DiagnosticCategory,
        line_map: &LineMap,
        content: &str,
        include_line_position: bool,
    ) -> serde_json::Value {
        let start_pos = line_map.offset_to_position(start_offset, content);
        let end_pos = line_map.offset_to_position(start_offset + length, content);
        let cat_str = match category {
            DiagnosticCategory::Error => "error",
            DiagnosticCategory::Warning => "warning",
            _ => "suggestion",
        };

        if include_line_position {
            // When includeLinePosition is true, the harness expects:
            // - start: 0-based byte offset (number)
            // - length: byte length (number)
            // - startLocation: {line, offset} (1-based)
            // - endLocation: {line, offset} (1-based)
            // - message: the diagnostic text
            // - category: category string
            // - code: error code
            serde_json::json!({
                "start": start_offset,
                "length": length,
                "startLocation": {
                    "line": start_pos.line + 1,
                    "offset": start_pos.character + 1,
                },
                "endLocation": {
                    "line": end_pos.line + 1,
                    "offset": end_pos.character + 1,
                },
                "message": message,
                "code": code,
                "category": cat_str,
            })
        } else {
            // Traditional tsserver format: start/end as {line, offset}
            serde_json::json!({
                "start": {
                    "line": start_pos.line + 1,
                    "offset": start_pos.character + 1,
                },
                "end": {
                    "line": end_pos.line + 1,
                    "offset": end_pos.character + 1,
                },
                "text": message,
                "code": code,
                "category": cat_str,
            })
        }
    }

    // Stub handlers for protocol commands - return success with empty/minimal responses
    pub(crate) fn stub_response(
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
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let interner = TypeInterner::new();
            let provider = HoverProvider::new(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.clone(),
            );
            let mut type_cache = None;
            let info = provider.get_hover(root, position, &mut type_cache)?;

            // Use structured fields from HoverInfo when available,
            // falling back to parsing from markdown contents
            let display_string = if !info.display_string.is_empty() {
                info.display_string.clone()
            } else {
                info.contents
                    .iter()
                    .find(|c| c.contains("```"))
                    .map(|c| {
                        c.replace("```typescript\n", "")
                            .replace("\n```", "")
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_default()
            };

            let documentation = if !info.documentation.is_empty() {
                info.documentation.clone()
            } else {
                info.contents
                    .iter()
                    .find(|c| !c.contains("```"))
                    .cloned()
                    .unwrap_or_default()
            };

            let kind = if !info.kind.is_empty() {
                info.kind.clone()
            } else {
                "unknown".to_string()
            };

            let kind_modifiers = info.kind_modifiers.clone();

            let range = info.range.unwrap_or(Range::new(position, position));
            // Build tags array from JSDoc tags when available
            let tags: Vec<serde_json::Value> = info
                .tags
                .iter()
                .map(|tag| {
                    serde_json::json!({
                        "name": tag.name,
                        "text": tag.text,
                    })
                })
                .collect();

            // Return documentation as a structured display parts array when non-empty,
            // or empty string when there's no documentation. The SessionClient handles
            // string documentation by wrapping in [{kind:"text", text:doc}].
            // When doc is "", that creates [{kind:"text",text:""}] (length 1) which
            // causes an unwanted blank line in baseline output.
            // Return as empty array [] to avoid the blank line.
            let doc_value: serde_json::Value = if documentation.is_empty() {
                serde_json::json!([])
            } else {
                serde_json::json!([{"kind": "text", "text": documentation}])
            };

            Some(serde_json::json!({
                "displayString": display_string,
                "documentation": doc_value,
                "kind": kind,
                "kindModifiers": kind_modifiers,
                "tags": tags,
                "start": Self::lsp_to_tsserver_position(&range.start),
                "end": Self::lsp_to_tsserver_position(&range.end),
            }))
        })();

        // When quickinfo fails to resolve, return a response with valid start/end
        // spans. The harness accesses body.start.line and body.end.line, so an
        // empty object {} would cause "Cannot read properties of undefined".
        let fallback = (|| -> Option<serde_json::Value> {
            let (_, line, offset) = Self::extract_file_position(&request.arguments)?;
            let position = Self::tsserver_to_lsp_position(line, offset);
            Some(serde_json::json!({
                "displayString": "",
                "documentation": "",
                "kind": "",
                "kindModifiers": "",
                "tags": [],
                "start": Self::lsp_to_tsserver_position(&position),
                "end": Self::lsp_to_tsserver_position(&position),
            }))
        })();
        self.stub_response(
            seq,
            request,
            result.or(fallback).or(Some(serde_json::json!({
                "displayString": "",
                "documentation": "",
                "kind": "",
                "kindModifiers": "",
                "tags": [],
                "start": {"line": 1, "offset": 1},
                "end": {"line": 1, "offset": 1},
            }))),
        )
    }

    fn handle_definition(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let locations = provider.get_definition(root, position)?;
            let body: Vec<serde_json::Value> = locations
                .iter()
                .map(|loc| {
                    serde_json::json!({
                        "file": loc.file_path,
                        "start": Self::lsp_to_tsserver_position(&loc.range.start),
                        "end": Self::lsp_to_tsserver_position(&loc.range.end),
                    })
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn handle_definition_and_bound_span(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let locations = provider.get_definition(root, position)?;

            // Build definitions array
            let definitions: Vec<serde_json::Value> = locations
                .iter()
                .map(|loc| {
                    serde_json::json!({
                        "file": loc.file_path,
                        "start": Self::lsp_to_tsserver_position(&loc.range.start),
                        "end": Self::lsp_to_tsserver_position(&loc.range.end),
                    })
                })
                .collect();

            // Compute the textSpan for the word at the cursor position
            let ref_offset = line_map.position_to_offset(position, &source_text)?;
            let node_idx = wasm::lsp::utils::find_node_at_offset(&arena, ref_offset);
            let text_span = if !node_idx.is_none() {
                if let Some(node) = arena.get(node_idx) {
                    let start_pos = line_map.offset_to_position(node.pos, &source_text);
                    let end_pos = line_map.offset_to_position(node.end, &source_text);
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(&start_pos),
                        "end": Self::lsp_to_tsserver_position(&end_pos),
                    })
                } else {
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(&position),
                        "end": Self::lsp_to_tsserver_position(&position),
                    })
                }
            } else {
                serde_json::json!({
                    "start": Self::lsp_to_tsserver_position(&position),
                    "end": Self::lsp_to_tsserver_position(&position),
                })
            };

            Some(serde_json::json!({
                "definitions": definitions,
                "textSpan": text_span,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "definitions": [],
                "textSpan": {"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}
            }))),
        )
    }

    fn handle_references(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let locations = provider.find_references(root, position)?;

            // Try to get symbol name from the position
            let symbol_name = {
                let ref_offset = line_map.position_to_offset(position, &source_text)?;
                let node_idx = wasm::lsp::utils::find_node_at_offset(&arena, ref_offset);
                if !node_idx.is_none() {
                    arena.get_identifier_text(node_idx).map(|s| s.to_string())
                } else {
                    None
                }
            }
            .unwrap_or_default();

            let refs: Vec<serde_json::Value> = locations
                .iter()
                .map(|loc| {
                    // Get line text for the reference
                    let line_text = source_text
                        .lines()
                        .nth(loc.range.start.line as usize)
                        .unwrap_or("")
                        .to_string();
                    serde_json::json!({
                        "file": loc.file_path,
                        "start": Self::lsp_to_tsserver_position(&loc.range.start),
                        "end": Self::lsp_to_tsserver_position(&loc.range.end),
                        "lineText": line_text,
                        "isWriteAccess": false,
                        "isDefinition": false,
                    })
                })
                .collect();
            Some(serde_json::json!({
                "refs": refs,
                "symbolName": symbol_name,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"refs": [], "symbolName": ""}))),
        )
    }

    fn handle_completions(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let interner = TypeInterner::new();
            let provider = Completions::new_with_types(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.clone(),
            );
            let completion_result = provider.get_completion_result(root, position)?;

            let entries: Vec<serde_json::Value> = completion_result
                .entries
                .iter()
                .map(|item| {
                    let kind = Self::completion_kind_to_str(item.kind);
                    // Map internal sort_text to tsserver's SortText format
                    let tsserver_sort_text = match item.sort_text.as_deref() {
                        Some("0") => "11", // LOCAL_DECLARATION -> LocationPriority
                        Some("1") => "11", // MEMBER -> LocationPriority
                        Some("2") => "11", // TYPE_DECLARATION -> LocationPriority
                        Some("3") => "16", // AUTO_IMPORT -> AutoImportSuggestions
                        Some("5") => "15", // KEYWORD -> GlobalsOrKeywords
                        _ => "11",         // Default to LocationPriority
                    };
                    let mut entry = serde_json::json!({
                        "name": item.label,
                        "kind": kind,
                        "sortText": tsserver_sort_text,
                    });
                    if let Some(ref modifiers) = item.kind_modifiers {
                        entry["kindModifiers"] = serde_json::json!(modifiers);
                    } else if let Some(ref detail) = item.detail {
                        entry["kindModifiers"] = serde_json::json!(detail);
                    }
                    entry
                })
                .collect();

            Some(serde_json::json!({
                "isGlobalCompletion": completion_result.is_global_completion,
                "isMemberCompletion": completion_result.is_member_completion,
                "isNewIdentifierLocation": completion_result.is_new_identifier_location,
                "entries": entries,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "isGlobalCompletion": false,
                "isMemberCompletion": false,
                "isNewIdentifierLocation": false,
                "entries": []
            }))),
        )
    }

    fn handle_completion_details(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let entry_names = request.arguments.get("entryNames")?.as_array()?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let interner = TypeInterner::new();
            let provider = Completions::new_with_types(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.to_string(),
            );
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64()? as u32;
            let position = Self::tsserver_to_lsp_position(line, offset);
            let items = provider.get_completions(root, position).unwrap_or_default();
            let details: Vec<serde_json::Value> = entry_names
                .iter()
                .map(|entry_name| {
                    let name = if let Some(s) = entry_name.as_str() {
                        s.to_string()
                    } else if let Some(obj) = entry_name.as_object() {
                        obj.get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string()
                    } else {
                        String::new()
                    };
                    // Try to find the matching completion item
                    let item = items.iter().find(|i| i.label == name);
                    let kind = item
                        .map(|i| Self::completion_kind_to_str(i.kind))
                        .unwrap_or("property");
                    let display_parts = if let Some(i) = item {
                        if let Some(ref detail) = i.detail {
                            serde_json::json!([{"text": detail, "kind": "text"}])
                        } else {
                            serde_json::json!([{"text": &name, "kind": "text"}])
                        }
                    } else {
                        serde_json::json!([{"text": &name, "kind": "text"}])
                    };
                    let documentation = item
                        .and_then(|i| i.documentation.as_ref())
                        .map(|doc| serde_json::json!([{"text": doc, "kind": "text"}]))
                        .unwrap_or(serde_json::json!([]));
                    serde_json::json!({
                        "name": name,
                        "kind": kind,
                        "kindModifiers": "",
                        "displayParts": display_parts,
                        "documentation": documentation,
                        "tags": [],
                        "codeActions": [],
                        "source": [],
                    })
                })
                .collect();
            Some(serde_json::json!(details))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn handle_signature_help(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let interner = TypeInterner::new();
            let provider = SignatureHelpProvider::new(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.clone(),
            );
            let mut type_cache = None;
            let sig_help = provider.get_signature_help(root, position, &mut type_cache)?;
            let items: Vec<serde_json::Value> = sig_help
                .signatures
                .iter()
                .map(|sig| {
                    let params: Vec<serde_json::Value> = sig
                        .parameters
                        .iter()
                        .map(|p| {
                            let mut param = serde_json::json!({
                                "name": p.name,
                                "display": [{"text": &p.label, "kind": "text"}],
                                "displayParts": [{"text": &p.label, "kind": "text"}],
                                "isOptional": p.is_optional,
                                "isRest": p.is_rest,
                            });
                            if let Some(ref doc) = p.documentation {
                                param["documentation"] =
                                    serde_json::json!([{"text": doc, "kind": "text"}]);
                            } else {
                                param["documentation"] = serde_json::json!([]);
                            }
                            param
                        })
                        .collect();
                    let mut item = serde_json::json!({
                        "isVariadic": sig.is_variadic,
                        "prefixDisplayParts": [{"text": &sig.prefix, "kind": "text"}],
                        "suffixDisplayParts": [{"text": &sig.suffix, "kind": "text"}],
                        "separatorDisplayParts": [{"text": ", ", "kind": "punctuation"}],
                        "parameters": params,
                        "tags": [],
                    });
                    if let Some(ref doc) = sig.documentation {
                        item["documentation"] = serde_json::json!([{"text": doc, "kind": "text"}]);
                    } else {
                        item["documentation"] = serde_json::json!([]);
                    }
                    item
                })
                .collect();
            Some(serde_json::json!({
                "items": items,
                "applicableSpan": {
                    "start": Self::lsp_to_tsserver_position(&position),
                    "end": Self::lsp_to_tsserver_position(&position),
                },
                "selectedItemIndex": sig_help.active_signature,
                "argumentIndex": sig_help.active_parameter,
                "argumentCount": sig_help.active_parameter + 1,
            }))
        })();
        // Always return a body - processResponse asserts !!response.body.
        // When no signature help is found, return empty items array.
        // The test-worker converts empty items to undefined.
        let body = result.unwrap_or_else(|| {
            serde_json::json!({
                "items": [],
                "applicableSpan": { "start": { "line": 1, "offset": 1 }, "end": { "line": 1, "offset": 1 } },
                "selectedItemIndex": 0,
                "argumentIndex": 0,
                "argumentCount": 0,
            })
        });
        self.stub_response(seq, request, Some(body))
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
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
            let symbols = provider.get_document_symbols(root);

            fn symbol_to_navtree(
                sym: &wasm::lsp::document_symbols::DocumentSymbol,
            ) -> serde_json::Value {
                let kind = match sym.kind {
                    wasm::lsp::document_symbols::SymbolKind::File => "module",
                    wasm::lsp::document_symbols::SymbolKind::Module => "module",
                    wasm::lsp::document_symbols::SymbolKind::Namespace => "module",
                    wasm::lsp::document_symbols::SymbolKind::Class => "class",
                    wasm::lsp::document_symbols::SymbolKind::Method => "method",
                    wasm::lsp::document_symbols::SymbolKind::Property => "property",
                    wasm::lsp::document_symbols::SymbolKind::Field => "property",
                    wasm::lsp::document_symbols::SymbolKind::Constructor => "constructor",
                    wasm::lsp::document_symbols::SymbolKind::Enum => "enum",
                    wasm::lsp::document_symbols::SymbolKind::Interface => "interface",
                    wasm::lsp::document_symbols::SymbolKind::Function => "function",
                    wasm::lsp::document_symbols::SymbolKind::Variable => "var",
                    wasm::lsp::document_symbols::SymbolKind::Constant => "const",
                    wasm::lsp::document_symbols::SymbolKind::EnumMember => "enum member",
                    wasm::lsp::document_symbols::SymbolKind::TypeParameter => "type parameter",
                    wasm::lsp::document_symbols::SymbolKind::Struct => "type",
                    _ => "unknown",
                };
                let children: Vec<serde_json::Value> =
                    sym.children.iter().map(symbol_to_navtree).collect();
                serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "childItems": children,
                    "spans": [{
                        "start": {
                            "line": sym.range.start.line + 1,
                            "offset": sym.range.start.character + 1,
                        },
                        "end": {
                            "line": sym.range.end.line + 1,
                            "offset": sym.range.end.character + 1,
                        },
                    }],
                })
            }

            let child_items: Vec<serde_json::Value> =
                symbols.iter().map(symbol_to_navtree).collect();

            // Compute the end span based on source text length
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map(|l| l.len()).unwrap_or(0);
            Some(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": child_items,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}}],
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": [],
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}],
            }))),
        )
    }

    fn handle_navbar(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
            let symbols = provider.get_document_symbols(root);

            fn symbol_to_navbar_item(
                sym: &wasm::lsp::document_symbols::DocumentSymbol,
                indent: usize,
                items: &mut Vec<serde_json::Value>,
            ) {
                let kind = match sym.kind {
                    wasm::lsp::document_symbols::SymbolKind::File => "module",
                    wasm::lsp::document_symbols::SymbolKind::Module => "module",
                    wasm::lsp::document_symbols::SymbolKind::Namespace => "module",
                    wasm::lsp::document_symbols::SymbolKind::Class => "class",
                    wasm::lsp::document_symbols::SymbolKind::Method => "method",
                    wasm::lsp::document_symbols::SymbolKind::Property => "property",
                    wasm::lsp::document_symbols::SymbolKind::Field => "property",
                    wasm::lsp::document_symbols::SymbolKind::Constructor => "constructor",
                    wasm::lsp::document_symbols::SymbolKind::Enum => "enum",
                    wasm::lsp::document_symbols::SymbolKind::Interface => "interface",
                    wasm::lsp::document_symbols::SymbolKind::Function => "function",
                    wasm::lsp::document_symbols::SymbolKind::Variable => "var",
                    wasm::lsp::document_symbols::SymbolKind::Constant => "const",
                    wasm::lsp::document_symbols::SymbolKind::EnumMember => "enum member",
                    wasm::lsp::document_symbols::SymbolKind::TypeParameter => "type parameter",
                    wasm::lsp::document_symbols::SymbolKind::Struct => "type",
                    _ => "unknown",
                };
                let child_items: Vec<serde_json::Value> = sym
                    .children
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "text": c.name,
                            "kind": match c.kind {
                                wasm::lsp::document_symbols::SymbolKind::Function => "function",
                                wasm::lsp::document_symbols::SymbolKind::Class => "class",
                                wasm::lsp::document_symbols::SymbolKind::Method => "method",
                                wasm::lsp::document_symbols::SymbolKind::Property => "property",
                                wasm::lsp::document_symbols::SymbolKind::Variable => "var",
                                wasm::lsp::document_symbols::SymbolKind::Constant => "const",
                                wasm::lsp::document_symbols::SymbolKind::Enum => "enum",
                                wasm::lsp::document_symbols::SymbolKind::Interface => "interface",
                                wasm::lsp::document_symbols::SymbolKind::EnumMember => "enum member",
                                wasm::lsp::document_symbols::SymbolKind::Struct => "type",
                                _ => "unknown",
                            },
                        })
                    })
                    .collect();
                items.push(serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "childItems": child_items,
                    "indent": indent,
                    "spans": [{
                        "start": {
                            "line": sym.range.start.line + 1,
                            "offset": sym.range.start.character + 1,
                        },
                        "end": {
                            "line": sym.range.end.line + 1,
                            "offset": sym.range.end.character + 1,
                        },
                    }],
                }));
                for child in &sym.children {
                    symbol_to_navbar_item(child, indent + 1, items);
                }
            }

            let mut items = Vec::new();
            // Root item
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map(|l| l.len()).unwrap_or(0);
            let child_items: Vec<serde_json::Value> = symbols
                .iter()
                .map(|sym| {
                    serde_json::json!({
                        "text": sym.name,
                        "kind": match sym.kind {
                            wasm::lsp::document_symbols::SymbolKind::Function => "function",
                            wasm::lsp::document_symbols::SymbolKind::Class => "class",
                            wasm::lsp::document_symbols::SymbolKind::Method => "method",
                            wasm::lsp::document_symbols::SymbolKind::Property => "property",
                            wasm::lsp::document_symbols::SymbolKind::Variable => "var",
                            wasm::lsp::document_symbols::SymbolKind::Constant => "const",
                            wasm::lsp::document_symbols::SymbolKind::Enum => "enum",
                            wasm::lsp::document_symbols::SymbolKind::Interface => "interface",
                            wasm::lsp::document_symbols::SymbolKind::EnumMember => "enum member",
                            _ => "unknown",
                        },
                    })
                })
                .collect();
            items.push(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": child_items,
                "indent": 0,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}}],
            }));
            // Flatten children
            for sym in &symbols {
                symbol_to_navbar_item(sym, 1, &mut items);
            }
            Some(serde_json::json!(items))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!([{
                "text": "<global>",
                "kind": "script",
                "childItems": [],
                "indent": 0,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}],
            }]))),
        )
    }

    fn handle_document_highlights(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider = DocumentHighlightProvider::new(&arena, &binder, &line_map, &source_text);
            let highlights = provider.get_document_highlights(root, position)?;

            // Group highlights by file (tsserver groups by file, each with highlightSpans)
            let highlight_spans: Vec<serde_json::Value> = highlights
                .iter()
                .map(|hl| {
                    let kind_str = match hl.kind {
                        Some(wasm::lsp::highlighting::DocumentHighlightKind::Read) => "reference",
                        Some(wasm::lsp::highlighting::DocumentHighlightKind::Write) => {
                            "writtenReference"
                        }
                        Some(wasm::lsp::highlighting::DocumentHighlightKind::Text) => "none",
                        None => "none",
                    };
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(&hl.range.start),
                        "end": Self::lsp_to_tsserver_position(&hl.range.end),
                        "kind": kind_str,
                    })
                })
                .collect();
            // All highlights are in the same file for now
            Some(serde_json::json!([{
                "file": file,
                "highlightSpans": highlight_spans,
            }]))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn handle_rename(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let new_name = request
                .arguments
                .get("findInStrings")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    // The actual tsserver rename command expects the new name from
                    // a separate "rename" request, not from the initial "rename" command.
                    // The initial command returns info + locations, not the edits.
                    None
                });
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                RenameProvider::new(&arena, &binder, &line_map, file.clone(), &source_text);

            // First check if rename is possible (prepare)
            let rename_range = provider.prepare_rename(position);
            if rename_range.is_none() {
                return Some(serde_json::json!({
                    "info": {
                        "canRename": false,
                        "localizedErrorMessage": "You cannot rename this element."
                    },
                    "locs": []
                }));
            }

            // If new_name is provided, perform the rename; otherwise just return locations
            if let Some(new_name) = new_name {
                match provider.provide_rename_edits(root, position, new_name.to_string()) {
                    Ok(edit) => {
                        let locs: Vec<serde_json::Value> = edit
                            .changes
                            .iter()
                            .map(|(file_path, edits)| {
                                let file_locs: Vec<serde_json::Value> = edits
                                    .iter()
                                    .map(|e| {
                                        serde_json::json!({
                                            "start": Self::lsp_to_tsserver_position(&e.range.start),
                                            "end": Self::lsp_to_tsserver_position(&e.range.end),
                                        })
                                    })
                                    .collect();
                                serde_json::json!({
                                    "file": file_path,
                                    "locs": file_locs,
                                })
                            })
                            .collect();
                        Some(serde_json::json!({
                            "info": { "canRename": true, "displayName": new_name, "fullDisplayName": new_name, "kind": "unknown", "kindModifiers": "", "triggerSpan": { "start": Self::lsp_to_tsserver_position(&rename_range.as_ref().unwrap().start), "length": 0 }},
                            "locs": locs,
                        }))
                    }
                    Err(msg) => Some(serde_json::json!({
                        "info": { "canRename": false, "localizedErrorMessage": msg },
                        "locs": []
                    })),
                }
            } else {
                // No new name - just return rename info with locations from references
                let find_refs =
                    FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
                let locations = find_refs.find_references(root, position);
                let file_locs: Vec<serde_json::Value> = locations
                    .unwrap_or_default()
                    .iter()
                    .map(|loc| {
                        serde_json::json!({
                            "start": Self::lsp_to_tsserver_position(&loc.range.start),
                            "end": Self::lsp_to_tsserver_position(&loc.range.end),
                        })
                    })
                    .collect();
                let span = rename_range.as_ref().unwrap();
                Some(serde_json::json!({
                    "info": {
                        "canRename": true,
                        "displayName": "",
                        "fullDisplayName": "",
                        "kind": "unknown",
                        "kindModifiers": "",
                        "triggerSpan": {
                            "start": Self::lsp_to_tsserver_position(&span.start),
                            "length": 0
                        }
                    },
                    "locs": [{
                        "file": file,
                        "locs": file_locs,
                    }]
                }))
            }
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "info": {"canRename": false, "localizedErrorMessage": "Not yet implemented"},
                "locs": []
            }))),
        )
    }

    fn handle_get_code_fixes(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?.to_string();
            let error_codes: Vec<u32> = request
                .arguments
                .get("errorCodes")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as u32))
                        .collect()
                })
                .unwrap_or_default();

            if error_codes.is_empty() {
                // If no specific error codes, get all diagnostics for the file
                // and return fixes for all of them
                let content = self.open_files.get(&file)?.clone();
                let diags = self.get_semantic_diagnostics_full(&file, &content);
                let mut all_fixes = Vec::new();
                let mut seen_codes = rustc_hash::FxHashSet::default();
                for diag in &diags {
                    if !seen_codes.insert(diag.code) {
                        continue;
                    }
                    let fixes =
                        wasm::lsp::code_actions::CodeFixRegistry::fixes_for_error_code(diag.code);
                    for (fix_name, fix_id, description, fix_all_desc) in fixes {
                        all_fixes.push(serde_json::json!({
                            "fixName": fix_name,
                            "description": description,
                            "changes": [{
                                "fileName": file,
                                "textChanges": []
                            }],
                            "fixId": fix_id,
                            "fixAllDescription": fix_all_desc
                        }));
                    }
                }
                return Some(serde_json::json!(all_fixes));
            }

            let mut all_fixes = Vec::new();
            for error_code in &error_codes {
                let fixes =
                    wasm::lsp::code_actions::CodeFixRegistry::fixes_for_error_code(*error_code);
                for (fix_name, fix_id, description, fix_all_desc) in fixes {
                    all_fixes.push(serde_json::json!({
                        "fixName": fix_name,
                        "description": description,
                        "changes": [{
                            "fileName": file,
                            "textChanges": []
                        }],
                        "commands": [],
                        "fixId": fix_id,
                        "fixAllDescription": fix_all_desc
                    }));
                }
            }
            Some(serde_json::json!(all_fixes))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn handle_get_combined_code_fix(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!({"changes": []})))
    }

    fn handle_references_full(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let locations = provider.find_references(root, position)?;
            let symbol_name = {
                let ref_offset = line_map.position_to_offset(position, &source_text)?;
                let node_idx = wasm::lsp::utils::find_node_at_offset(&arena, ref_offset);
                if !node_idx.is_none() {
                    arena.get_identifier_text(node_idx).map(|s| s.to_string())
                } else {
                    None
                }
            }
            .unwrap_or_default();
            let refs: Vec<serde_json::Value> = locations
                .iter()
                .map(|loc| {
                    let line_text = source_text
                        .lines()
                        .nth(loc.range.start.line as usize)
                        .unwrap_or("")
                        .to_string();
                    serde_json::json!({
                        "file": loc.file_path,
                        "start": Self::lsp_to_tsserver_position(&loc.range.start),
                        "end": Self::lsp_to_tsserver_position(&loc.range.end),
                        "lineText": line_text,
                        "isWriteAccess": false,
                        "isDefinition": false,
                    })
                })
                .collect();
            Some(serde_json::json!({
                "refs": refs,
                "symbolName": symbol_name,
                "symbolStartOffset": offset,
                "symbolDisplayString": symbol_name,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "refs": [],
                "symbolName": "",
                "symbolStartOffset": 0,
                "symbolDisplayString": ""
            }))),
        )
    }

    fn handle_navto(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let search_value = request
                .arguments
                .get("searchValue")
                .and_then(|v| v.as_str())?;
            if search_value.is_empty() {
                return Some(serde_json::json!([]));
            }
            let search_lower = search_value.to_lowercase();
            let mut nav_items: Vec<serde_json::Value> = Vec::new();
            let file_paths: Vec<String> = self.open_files.keys().cloned().collect();
            for file_path in &file_paths {
                if let Some((arena, _binder, root, source_text)) =
                    self.parse_and_bind_file(file_path)
                {
                    let line_map = LineMap::build(&source_text);
                    let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
                    let symbols = provider.get_document_symbols(root);
                    Self::collect_navto_items(
                        &symbols,
                        search_value,
                        &search_lower,
                        file_path,
                        &mut nav_items,
                    );
                }
            }
            Some(serde_json::json!(nav_items))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn collect_navto_items(
        symbols: &[wasm::lsp::document_symbols::DocumentSymbol],
        search_value: &str,
        search_lower: &str,
        file_path: &str,
        result: &mut Vec<serde_json::Value>,
    ) {
        for sym in symbols {
            let name_lower = sym.name.to_lowercase();
            if name_lower.contains(search_lower) {
                let is_case_sensitive = sym.name.contains(search_value);
                let kind = match sym.kind {
                    wasm::lsp::document_symbols::SymbolKind::Module => "module",
                    wasm::lsp::document_symbols::SymbolKind::Class => "class",
                    wasm::lsp::document_symbols::SymbolKind::Method => "method",
                    wasm::lsp::document_symbols::SymbolKind::Property => "property",
                    wasm::lsp::document_symbols::SymbolKind::Field => "property",
                    wasm::lsp::document_symbols::SymbolKind::Constructor => "constructor",
                    wasm::lsp::document_symbols::SymbolKind::Enum => "enum",
                    wasm::lsp::document_symbols::SymbolKind::Interface => "interface",
                    wasm::lsp::document_symbols::SymbolKind::Function => "function",
                    wasm::lsp::document_symbols::SymbolKind::Variable => "var",
                    wasm::lsp::document_symbols::SymbolKind::Constant => "const",
                    wasm::lsp::document_symbols::SymbolKind::EnumMember => "enum member",
                    wasm::lsp::document_symbols::SymbolKind::TypeParameter => "type parameter",
                    _ => "unknown",
                };
                let match_kind = if name_lower == *search_lower {
                    "exact"
                } else if name_lower.starts_with(search_lower) {
                    "prefix"
                } else {
                    "substring"
                };
                result.push(serde_json::json!({
                    "name": sym.name,
                    "kind": kind,
                    "kindModifiers": "",
                    "matchKind": match_kind,
                    "isCaseSensitive": is_case_sensitive,
                    "file": file_path,
                    "start": {
                        "line": sym.range.start.line + 1,
                        "offset": sym.range.start.character + 1,
                    },
                    "end": {
                        "line": sym.range.end.line + 1,
                        "offset": sym.range.end.character + 1,
                    },
                }));
            }
            Self::collect_navto_items(&sym.children, search_value, search_lower, file_path, result);
        }
    }

    fn handle_get_supported_code_fixes(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let codes: Vec<String> = wasm::lsp::code_actions::CodeFixRegistry::supported_error_codes()
            .iter()
            .map(|c| c.to_string())
            .collect();
        self.stub_response(seq, request, Some(serde_json::json!(codes)))
    }

    fn handle_encoded_semantic_classifications_full(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let mut provider =
                SemanticTokensProvider::new(&arena, &binder, &line_map, &source_text);
            let tokens = provider.get_semantic_tokens(root);
            Some(serde_json::json!({
                "spans": tokens,
                "endOfLineState": 0,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"spans": [], "endOfLineState": 0}))),
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
        self.stub_response(seq, request, Some(serde_json::json!({"edits": []})))
    }

    fn handle_organize_imports(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
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
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let source_text = self.open_files.get(file)?.clone();

            let options = wasm::lsp::formatting::FormattingOptions {
                tab_size: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("tabSize"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(4) as u32,
                insert_spaces: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("insertSpaces"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                ..Default::default()
            };

            match wasm::lsp::formatting::DocumentFormattingProvider::format_document(
                file,
                &source_text,
                &options,
            ) {
                Ok(edits) => {
                    let body: Vec<serde_json::Value> = edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(&edit.range.start),
                                "end": Self::lsp_to_tsserver_position(&edit.range.end),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    Some(serde_json::json!(body))
                }
                Err(_) => Some(serde_json::json!([])),
            }
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn handle_format_on_key(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let source_text = self.open_files.get(file)?.clone();
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64()? as u32;
            let key = request.arguments.get("key")?.as_str()?;

            let options = wasm::lsp::formatting::FormattingOptions {
                tab_size: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("tabSize"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(4) as u32,
                insert_spaces: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("insertSpaces"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                ..Default::default()
            };

            // tsserver protocol uses 1-based line/offset, convert to 0-based
            let lsp_line = line.saturating_sub(1);
            let lsp_offset = offset.saturating_sub(1);

            match wasm::lsp::formatting::DocumentFormattingProvider::format_on_key(
                &source_text,
                lsp_line,
                lsp_offset,
                key,
                &options,
            ) {
                Ok(edits) => {
                    let body: Vec<serde_json::Value> = edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(&edit.range.start),
                                "end": Self::lsp_to_tsserver_position(&edit.range.end),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    Some(serde_json::json!(body))
                }
                Err(_) => Some(serde_json::json!([])),
            }
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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

    fn handle_external_project(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(seq, request, None)
    }

    fn handle_update_open(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        // Process opened, changed, and closed files
        if let Some(opened) = request
            .arguments
            .get("openFiles")
            .and_then(|v| v.as_array())
        {
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
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let interner = TypeInterner::new();
            let provider = InlayHintsProvider::new(
                &arena,
                &binder,
                &line_map,
                &source_text,
                &interner,
                file.to_string(),
            );

            // Extract the range from arguments, default to entire file
            let start = request
                .arguments
                .get("start")
                .and_then(|v| v.as_u64())
                .map(|_| {
                    let line = request
                        .arguments
                        .get("startLine")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1) as u32;
                    let offset = request
                        .arguments
                        .get("startOffset")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1) as u32;
                    Self::tsserver_to_lsp_position(line, offset)
                })
                .unwrap_or(Position::new(0, 0));
            let end = request
                .arguments
                .get("end")
                .and_then(|v| v.as_u64())
                .map(|_| {
                    let line = request
                        .arguments
                        .get("endLine")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(u32::MAX as u64) as u32;
                    let offset = request
                        .arguments
                        .get("endOffset")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(u32::MAX as u64) as u32;
                    Self::tsserver_to_lsp_position(line, offset)
                })
                .unwrap_or(Position::new(u32::MAX, u32::MAX));
            let range = Range::new(start, end);

            let hints = provider.provide_inlay_hints(root, range);
            let body: Vec<serde_json::Value> = hints
                .iter()
                .map(|hint| {
                    let kind = match hint.kind {
                        wasm::lsp::inlay_hints::InlayHintKind::Parameter => "Parameter",
                        wasm::lsp::inlay_hints::InlayHintKind::Type => "Type",
                        wasm::lsp::inlay_hints::InlayHintKind::Generic => "Enum",
                    };
                    serde_json::json!({
                        "text": hint.label,
                        "position": Self::lsp_to_tsserver_position(&hint.position),
                        "kind": kind,
                        "whitespaceBefore": false,
                        "whitespaceAfter": true,
                    })
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn handle_selection_range(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, _root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = SelectionRangeProvider::new(&arena, &line_map, &source_text);

            let locations = request.arguments.get("locations")?.as_array()?;
            let positions: Vec<Position> = locations
                .iter()
                .filter_map(|loc| {
                    let line = loc.get("line")?.as_u64()? as u32;
                    let offset = loc.get("offset")?.as_u64()? as u32;
                    Some(Self::tsserver_to_lsp_position(line, offset))
                })
                .collect();

            let ranges = provider.get_selection_ranges(&positions);

            fn selection_range_to_json(
                sr: &wasm::lsp::selection_range::SelectionRange,
            ) -> serde_json::Value {
                let text_span = serde_json::json!({
                    "start": {
                        "line": sr.range.start.line + 1,
                        "offset": sr.range.start.character + 1,
                    },
                    "end": {
                        "line": sr.range.end.line + 1,
                        "offset": sr.range.end.character + 1,
                    },
                });
                if let Some(ref parent) = sr.parent {
                    serde_json::json!({
                        "textSpan": text_span,
                        "parent": selection_range_to_json(parent),
                    })
                } else {
                    serde_json::json!({
                        "textSpan": text_span,
                    })
                }
            }

            let body: Vec<serde_json::Value> = ranges
                .iter()
                .map(|opt_sr| {
                    opt_sr
                        .as_ref()
                        .map(selection_range_to_json)
                        .unwrap_or(serde_json::json!(null))
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                CallHierarchyProvider::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let item = provider.prepare(root, position)?;
            Some(serde_json::json!([{
                "name": item.name,
                "kind": format!("{:?}", item.kind).to_lowercase(),
                "file": item.uri,
                "span": {
                    "start": Self::lsp_to_tsserver_position(&item.range.start),
                    "end": Self::lsp_to_tsserver_position(&item.range.end),
                },
                "selectionSpan": {
                    "start": Self::lsp_to_tsserver_position(&item.selection_range.start),
                    "end": Self::lsp_to_tsserver_position(&item.selection_range.end),
                },
            }]))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn handle_call_hierarchy(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                CallHierarchyProvider::new(&arena, &binder, &line_map, file.clone(), &source_text);

            let is_incoming = request.command == "provideCallHierarchyIncomingCalls";

            if is_incoming {
                let calls = provider.incoming_calls(root, position);
                let body: Vec<serde_json::Value> = calls
                    .iter()
                    .map(|call| {
                        let from_ranges: Vec<serde_json::Value> = call
                            .from_ranges
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "start": Self::lsp_to_tsserver_position(&r.start),
                                    "end": Self::lsp_to_tsserver_position(&r.end),
                                })
                            })
                            .collect();
                        serde_json::json!({
                            "from": {
                                "name": call.from.name,
                                "kind": format!("{:?}", call.from.kind).to_lowercase(),
                                "file": call.from.uri,
                                "span": {
                                    "start": Self::lsp_to_tsserver_position(&call.from.range.start),
                                    "end": Self::lsp_to_tsserver_position(&call.from.range.end),
                                },
                                "selectionSpan": {
                                    "start": Self::lsp_to_tsserver_position(&call.from.selection_range.start),
                                    "end": Self::lsp_to_tsserver_position(&call.from.selection_range.end),
                                },
                            },
                            "fromSpans": from_ranges,
                        })
                    })
                    .collect();
                Some(serde_json::json!(body))
            } else {
                let calls = provider.outgoing_calls(root, position);
                let body: Vec<serde_json::Value> = calls
                    .iter()
                    .map(|call| {
                        let from_ranges: Vec<serde_json::Value> = call
                            .from_ranges
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "start": Self::lsp_to_tsserver_position(&r.start),
                                    "end": Self::lsp_to_tsserver_position(&r.end),
                                })
                            })
                            .collect();
                        serde_json::json!({
                            "to": {
                                "name": call.to.name,
                                "kind": format!("{:?}", call.to.kind).to_lowercase(),
                                "file": call.to.uri,
                                "span": {
                                    "start": Self::lsp_to_tsserver_position(&call.to.range.start),
                                    "end": Self::lsp_to_tsserver_position(&call.to.range.end),
                                },
                                "selectionSpan": {
                                    "start": Self::lsp_to_tsserver_position(&call.to.selection_range.start),
                                    "end": Self::lsp_to_tsserver_position(&call.to.selection_range.end),
                                },
                            },
                            "fromSpans": from_ranges,
                        })
                    })
                    .collect();
                Some(serde_json::json!(body))
            }
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn handle_map_code(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn handle_file_references(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"refs": [], "symbolName": ""})),
        )
    }

    fn handle_implementation(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider = GoToImplementationProvider::new(
                &arena,
                &binder,
                &line_map,
                file.clone(),
                &source_text,
            );
            let locations = provider.get_implementations(root, position)?;
            let body: Vec<serde_json::Value> = locations
                .iter()
                .map(|loc| {
                    serde_json::json!({
                        "file": loc.file_path,
                        "start": Self::lsp_to_tsserver_position(&loc.range.start),
                        "end": Self::lsp_to_tsserver_position(&loc.range.end),
                    })
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn handle_outlining_spans(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = FoldingRangeProvider::new(&arena, &line_map, &source_text);
            let ranges = provider.get_folding_ranges(root);

            // Pre-compute line lengths for offset calculation
            let lines: Vec<&str> = source_text.lines().collect();

            let body: Vec<serde_json::Value> = ranges
                .iter()
                .map(|fr| {
                    // Compute end offset: length of the end line + 1 (1-based)
                    let end_line_len = lines
                        .get(fr.end_line as usize)
                        .map(|l| l.len() as u32)
                        .unwrap_or(0);
                    let start_line_len = lines
                        .get(fr.start_line as usize)
                        .map(|l| l.len() as u32)
                        .unwrap_or(0);

                    let mut span = serde_json::json!({
                        "textSpan": {
                            "start": { "line": fr.start_line + 1, "offset": 1 },
                            "end": { "line": fr.end_line + 1, "offset": end_line_len + 1 },
                        },
                        "hintSpan": {
                            "start": { "line": fr.start_line + 1, "offset": 1 },
                            "end": { "line": fr.start_line + 1, "offset": start_line_len + 1 },
                        },
                        "bannerText": "...",
                        "autoCollapse": false,
                    });
                    if let Some(ref kind) = fr.kind {
                        span["kind"] = serde_json::json!(kind);
                    }
                    span
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    // Editing handlers extracted to handlers_editing.rs

    // =========================================================================
    // Legacy Protocol Handling
    // =========================================================================

    fn handle_legacy_request(&mut self, request: LegacyRequest) -> LegacyResponse {
        match request {
            LegacyRequest::Check { id, files, options } => {
                self.handle_legacy_check(id, files, options)
            }
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

    /// Get full semantic diagnostics for a single file (with position info).
    fn get_semantic_diagnostics_full(
        &mut self,
        file_path: &str,
        content: &str,
    ) -> Vec<wasm::checker::types::diagnostics::Diagnostic> {
        let options = CheckOptions::default();

        let lib_files = match if options.no_lib {
            Ok(vec![])
        } else {
            self.load_libs(&options)
        } {
            Ok(libs) => libs,
            Err(_) => return Vec::new(),
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

        let mut parser = ParserState::new(file_path.to_string(), content.to_string());
        let root = parser.parse_source_file();
        let parse_diagnostics = parser.get_diagnostics().to_vec();
        let arena = Arc::new(parser.into_arena());
        let mut binder = BinderState::new();
        binder.bind_source_file(&arena, root);
        let binder = Arc::new(binder);

        let all_arenas: Vec<Arc<NodeArena>> = vec![arena.clone()];
        let all_binders: Vec<Arc<BinderState>> = vec![binder.clone()];
        let user_file_contexts: Vec<LibContext> = vec![LibContext {
            arena: arena.clone(),
            binder: binder.clone(),
        }];

        let mut all_contexts = lib_contexts;
        all_contexts.extend(user_file_contexts);

        let file_names = vec![file_path.to_string()];
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

        let query_cache = wasm::solver::QueryCache::new(&type_interner);

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

        let mut diagnostics: Vec<wasm::checker::types::diagnostics::Diagnostic> = Vec::new();

        // Add parse diagnostics
        for d in &parse_diagnostics {
            diagnostics.push(wasm::checker::types::diagnostics::Diagnostic::error(
                file_path.to_string(),
                d.start,
                d.length,
                d.message.clone(),
                d.code,
            ));
        }

        // Add checker diagnostics
        for diag in checker.ctx.diagnostics {
            if diag.category == DiagnosticCategory::Error {
                diagnostics.push(diag);
            }
        }

        diagnostics
    }

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
        let mut binary_file_errors: Vec<(String, i32)> = Vec::new();
        for (file_name, content) in &files {
            // Skip non-TypeScript/JavaScript files (e.g. .json, .txt).
            // They may be present in multi-file tests for module resolution
            // fixtures but must not be parsed as TypeScript source.
            if !Self::is_checkable_file(file_name) {
                continue;
            }

            // Check if content appears to be garbled binary (e.g., UTF-16 read as UTF-8)
            // If so, emit TS1490 "File appears to be binary." instead of parsing
            if content_appears_binary(content) {
                binary_file_errors.push((file_name.clone(), TS1490_FILE_APPEARS_TO_BE_BINARY));
                continue;
            }

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
        let all_arenas: Vec<Arc<NodeArena>> = bound_files.iter().map(|f| f.arena.clone()).collect();
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
        let query_cache = wasm::solver::QueryCache::new(&type_interner);
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

            if !all_contexts.is_empty() {
                checker.ctx.set_lib_contexts(all_contexts.clone());
            }
            // Set the count of actual lib files (not user files) for has_lib_loaded()
            checker.ctx.set_actual_lib_file_count(lib_files.len());

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

        // No embedded libs fallback - lib files must be on disk (matching tsgo behavior)
        // Users need TypeScript installed or TSZ_LIB_DIR set
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

    /// Returns true if the file has a TypeScript or JavaScript extension that
    /// should be parsed and type-checked. Non-source files (.json, .txt, etc.)
    /// that appear in multi-file test fixtures should be skipped.
    fn is_checkable_file(file_name: &str) -> bool {
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
            allow_synthetic_default_imports: options
                .allow_synthetic_default_imports
                .unwrap_or(options.es_module_interop),
            allow_unreachable_code: options.allow_unreachable_code.unwrap_or(false),
            no_property_access_from_index_signature: options
                .no_property_access_from_index_signature,
            sound_mode: false, // Sound mode not yet exposed in server protocol
            experimental_decorators: options.experimental_decorators,
            no_unused_locals: options.no_unused_locals,
            no_unused_parameters: options.no_unused_parameters,
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
        len_str
            .trim()
            .parse::<usize>()
            .with_context(|| format!("invalid Content-Length: {}", len_str.trim()))?
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
    write!(
        stdout,
        "Content-Length: {}\r\n\r\n{}",
        message.len(),
        message
    )?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_server() -> Server {
        Server {
            lib_dir: PathBuf::from("/nonexistent"),
            tests_lib_dir: PathBuf::from("/nonexistent"),
            lib_cache: FxHashMap::default(),
            checks_completed: 0,
            response_seq: 0,
            open_files: HashMap::new(),
            _server_mode: ServerMode::Semantic,
            _log_config: LogConfig {
                level: LogLevel::Off,
                file: None,
                trace_to_console: false,
            },
        }
    }

    fn make_request(command: &str, arguments: serde_json::Value) -> TsServerRequest {
        TsServerRequest {
            seq: 1,
            msg_type: "request".to_string(),
            command: command.to_string(),
            arguments,
        }
    }

    #[test]
    fn test_line_offset_to_byte_first_char() {
        assert_eq!(Server::line_offset_to_byte("hello\nworld\n", 1, 1), 0);
    }

    #[test]
    fn test_line_offset_to_byte_second_line() {
        assert_eq!(Server::line_offset_to_byte("hello\nworld\n", 2, 1), 6);
    }

    #[test]
    fn test_apply_change_insert() {
        assert_eq!(
            Server::apply_change("hello world", 1, 7, 1, 7, "beautiful "),
            "hello beautiful world"
        );
    }

    #[test]
    fn test_apply_change_replace() {
        assert_eq!(
            Server::apply_change("hello world", 1, 7, 1, 12, "Rust"),
            "hello Rust"
        );
    }

    #[test]
    fn test_apply_change_delete() {
        assert_eq!(
            Server::apply_change("hello world", 1, 7, 1, 12, ""),
            "hello "
        );
    }

    #[test]
    fn test_handle_change_updates_file() {
        let mut server = make_server();
        server
            .open_files
            .insert("/test.ts".to_string(), "const x = 1;".to_string());
        let req = make_request(
            "change",
            serde_json::json!({
                "file": "/test.ts",
                "line": 1, "offset": 11,
                "endLine": 1, "endOffset": 12,
                "insertString": "2"
            }),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success);
        assert_eq!(server.open_files.get("/test.ts").unwrap(), "const x = 2;");
    }

    #[test]
    fn test_new_commands_are_recognized() {
        let mut server = make_server();
        let commands = vec![
            "change",
            "configure",
            "references-full",
            "navto",
            "signatureHelp",
            "completionEntryDetails",
            "getSupportedCodeFixes",
            "getApplicableRefactors",
            "getEditsForRefactor",
            "encodedSemanticClassifications-full",
            "breakpointStatement",
            "jsxClosingTag",
            "braceCompletion",
            "getSpanOfEnclosingComment",
            "todoComments",
            "docCommentTemplate",
            "indentation",
            "toggleLineComment",
            "toggleMultilineComment",
            "commentSelection",
            "uncommentSelection",
            "getSmartSelectionRange",
            "getSyntacticClassifications",
            "getSemanticClassifications",
            "getCompilerOptionsDiagnostics",
        ];
        for cmd in commands {
            let req = make_request(
                cmd,
                serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
            );
            let resp = server.handle_tsserver_request(req);
            assert!(
                resp.success
                    || !resp
                        .message
                        .as_deref()
                        .unwrap_or("")
                        .contains("Unrecognized"),
                "Command '{}' was not recognized",
                cmd
            );
        }
    }

    #[test]
    fn test_unrecognized_command() {
        let mut server = make_server();
        let req = make_request("nonExistentCommand", serde_json::json!({}));
        let resp = server.handle_tsserver_request(req);
        assert!(!resp.success);
        assert!(
            resp.message
                .unwrap()
                .contains("Unrecognized command: nonExistentCommand")
        );
    }

    /// Helper to validate that a JSON value has valid tsserver start/end spans.
    fn assert_valid_span(value: &serde_json::Value, context: &str) {
        let start = value.get("start");
        assert!(start.is_some(), "{}: missing 'start' field", context);
        let start = start.unwrap();
        assert!(
            start.get("line").is_some(),
            "{}: missing 'start.line'",
            context
        );
        assert!(
            start.get("offset").is_some(),
            "{}: missing 'start.offset'",
            context
        );
        let line = start.get("line").unwrap().as_u64().unwrap();
        let offset = start.get("offset").unwrap().as_u64().unwrap();
        assert!(line >= 1, "{}: start.line must be >= 1 (1-based)", context);
        assert!(
            offset >= 1,
            "{}: start.offset must be >= 1 (1-based)",
            context
        );

        let end = value.get("end");
        assert!(end.is_some(), "{}: missing 'end' field", context);
        let end = end.unwrap();
        assert!(end.get("line").is_some(), "{}: missing 'end.line'", context);
        assert!(
            end.get("offset").is_some(),
            "{}: missing 'end.offset'",
            context
        );
        let end_line = end.get("line").unwrap().as_u64().unwrap();
        let end_offset = end.get("offset").unwrap().as_u64().unwrap();
        assert!(
            end_line >= 1,
            "{}: end.line must be >= 1 (1-based)",
            context
        );
        assert!(
            end_offset >= 1,
            "{}: end.offset must be >= 1 (1-based)",
            context
        );
    }

    #[test]
    fn test_quickinfo_response_always_has_valid_spans() {
        // When quickinfo is called on a valid symbol, the response body must
        // include start/end with line/offset fields.
        let mut server = make_server();
        server
            .open_files
            .insert("/test.ts".to_string(), "const x = 42;".to_string());
        let req = make_request(
            "quickinfo",
            serde_json::json!({"file": "/test.ts", "line": 1, "offset": 7}),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success);
        let body = resp.body.expect("quickinfo should return a body");
        assert_valid_span(&body, "quickinfo on valid symbol");
    }

    #[test]
    fn test_quickinfo_fallback_has_valid_spans() {
        // When quickinfo is called on whitespace or a position where no symbol
        // is found, the response body must still have start/end spans to avoid
        // "Cannot read properties of undefined (reading 'line')" in the harness.
        let mut server = make_server();
        server
            .open_files
            .insert("/test.ts".to_string(), "   ".to_string());
        let req = make_request(
            "quickinfo",
            serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success);
        let body = resp.body.expect("quickinfo fallback should return a body");
        assert_valid_span(&body, "quickinfo fallback on whitespace");
    }

    #[test]
    fn test_quickinfo_on_nonexistent_file_has_valid_spans() {
        // Even when the file is not open, the quickinfo fallback must return
        // valid span data.
        let mut server = make_server();
        let req = make_request(
            "quickinfo",
            serde_json::json!({"file": "/nonexistent.ts", "line": 1, "offset": 1}),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success);
        let body = resp.body.expect("quickinfo fallback should return a body");
        assert_valid_span(&body, "quickinfo on nonexistent file");
    }

    #[test]
    fn test_quickinfo_uses_hover_info_structured_fields() {
        // When HoverInfo returns structured kind/kindModifiers/displayString/
        // documentation fields, they should be used in the response instead of
        // being re-parsed from markdown contents.
        let mut server = make_server();
        server
            .open_files
            .insert("/test.ts".to_string(), "const myVar = 42;".to_string());
        let req = make_request(
            "quickinfo",
            serde_json::json!({"file": "/test.ts", "line": 1, "offset": 7}),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success);
        let body = resp.body.expect("quickinfo should return a body");
        // The body must have displayString, kind, kindModifiers, documentation
        assert!(
            body.get("displayString").is_some(),
            "quickinfo must have displayString"
        );
        assert!(body.get("kind").is_some(), "quickinfo must have kind");
        assert!(
            body.get("kindModifiers").is_some(),
            "quickinfo must have kindModifiers"
        );
        assert!(
            body.get("documentation").is_some(),
            "quickinfo must have documentation"
        );
    }

    #[test]
    fn test_definition_response_entries_have_valid_spans() {
        // Each definition entry in the response must have start/end spans with
        // valid line/offset fields.
        let mut server = make_server();
        server.open_files.insert(
            "/test.ts".to_string(),
            "const x = 1;\nx;".replace("\n", "\n").to_string(),
        );
        // Open file with actual newline
        server.open_files.insert(
            "/test.ts".to_string(),
            "const x = 1;
x;"
            .to_string(),
        );
        let req = make_request(
            "definition",
            serde_json::json!({"file": "/test.ts", "line": 2, "offset": 1}),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success);
        let body = resp.body.expect("definition should return a body");
        // The body is an array; each entry must have start/end and file
        if let Some(arr) = body.as_array() {
            for (i, entry) in arr.iter().enumerate() {
                assert_valid_span(entry, &format!("definition entry {}", i));
                assert!(
                    entry.get("file").is_some(),
                    "definition entry {} must have 'file'",
                    i
                );
            }
        }
    }

    #[test]
    fn test_definition_empty_response_is_valid_array() {
        // When no definition is found, the response must be an empty array,
        // not null or an object missing start/end.
        let mut server = make_server();
        server
            .open_files
            .insert("/test.ts".to_string(), "   ".to_string());
        let req = make_request(
            "definition",
            serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success);
        let body = resp.body.expect("definition should return a body");
        assert!(body.is_array(), "definition fallback must be an array");
    }

    #[test]
    fn test_definition_and_bound_span_has_valid_text_span() {
        // The definitionAndBoundSpan response must always have a textSpan with
        // valid start/end, even when no definitions are found.
        let mut server = make_server();
        server
            .open_files
            .insert("/test.ts".to_string(), "   ".to_string());
        let req = make_request(
            "definitionAndBoundSpan",
            serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success);
        let body = resp
            .body
            .expect("definitionAndBoundSpan should return a body");
        let text_span = body
            .get("textSpan")
            .expect("definitionAndBoundSpan must have textSpan");
        assert_valid_span(text_span, "definitionAndBoundSpan textSpan");
        assert!(
            body.get("definitions").is_some(),
            "definitionAndBoundSpan must have definitions array"
        );
    }

    #[test]
    fn test_navtree_fallback_has_spans() {
        // The navtree/navbar fallback must include a spans array so the harness
        // does not crash when iterating item.spans.
        let mut server = make_server();
        let req = make_request("navtree", serde_json::json!({"file": "/nonexistent.ts"}));
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success);
        let body = resp.body.expect("navtree should return a body");
        let spans = body.get("spans");
        assert!(spans.is_some(), "navtree fallback must have spans array");
        let spans_arr = spans.unwrap().as_array().expect("spans must be an array");
        assert!(
            !spans_arr.is_empty(),
            "navtree fallback must have at least one span"
        );
        assert_valid_span(&spans_arr[0], "navtree fallback span");
    }

    #[test]
    fn test_references_response_entries_have_valid_spans() {
        // Each reference entry must have valid start/end spans.
        let mut server = make_server();
        server.open_files.insert(
            "/test.ts".to_string(),
            "const x = 1;
x;
x;"
            .to_string(),
        );
        let req = make_request(
            "references",
            serde_json::json!({"file": "/test.ts", "line": 1, "offset": 7}),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success);
        let body = resp.body.expect("references should return a body");
        let refs = body.get("refs").expect("references must have refs array");
        if let Some(arr) = refs.as_array() {
            for (i, entry) in arr.iter().enumerate() {
                assert_valid_span(entry, &format!("reference entry {}", i));
            }
        }
        assert!(
            body.get("symbolName").is_some(),
            "references must have symbolName"
        );
    }
}
