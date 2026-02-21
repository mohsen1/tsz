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

//! Environment variables (tsserver-compatible):
//!   `TSS_LOG`     - Configure logging (e.g., "-level verbose -file /tmp/tsserver.log")
//!   `TSS_DEBUG`   - Enable debug mode on specified port
//!   `TSS_DEBUG_BRK` - Enable debug mode with break on startup
//!
//! Legacy usage:
//! ```bash
//! echo '{"type":"check","id":1,"files":{"main.ts":"const x: string = 1;"}}' | tsz-server --protocol legacy
//! ```

mod check;
mod handlers_completions;
mod handlers_diagnostics;
mod handlers_editing;
mod handlers_files;
mod handlers_info;
mod handlers_legacy;
mod handlers_structure;

use anyhow::{Context, Result};
use clap::Parser;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read as IoRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

use tsz::binder::BinderState;
use tsz::lib_loader::LibFile;
use tsz::lsp::position::Position;
use tsz::parser::ParserState;
use tsz::parser::base::NodeIndex;
use tsz::parser::node::NodeArena;

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

    let control_count = check_slice
        .chars()
        .filter(|&ch| {
            let code = ch as u32;
            code <= 0x1f
                && code != 0x09
                && code != 0x0A
                && code != 0x0D
                && code != 0x0B
                && code != 0x0C
        })
        .count();
    if control_count >= 4 {
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

    /// Allow loading language service plugins from local project `node_modules`.
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
    /// If name ends with '*', actual pipe name is <`name_without`_*><requestId>.
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

    /// Enable project-wide `IntelliSense` in web context.
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
pub(crate) struct TsServerRequest {
    pub(crate) seq: u64,
    #[serde(rename = "type")]
    pub(crate) _msg_type: String,
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
        files: Box<FxHashMap<String, String>>,
        #[serde(default)]
        options: Box<CheckOptions>,
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
struct CheckOptions {
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
    no_implicit_any: Option<bool>,
    #[serde(default)]
    no_implicit_this: Option<bool>,
    #[serde(default)]
    no_implicit_returns: bool,
    #[serde(default)]
    use_unknown_in_catch_variables: Option<bool>,
    #[serde(default)]
    always_strict: Option<bool>,
    #[serde(default)]
    no_unused_locals: bool,
    #[serde(default)]
    no_unused_parameters: bool,
    #[serde(default)]
    exact_optional_property_types: bool,
    #[serde(default)]
    no_unchecked_indexed_access: bool,
    #[serde(default)]
    allow_unreachable_code: Option<bool>,
    #[serde(default)]
    no_property_access_from_index_signature: bool,
    #[serde(default)]
    es_module_interop: bool,
    #[serde(default)]
    allow_synthetic_default_imports: Option<bool>,
    #[serde(default)]
    isolated_modules: bool,
    #[serde(default)]
    no_lib: bool,
    #[serde(default)]
    lib: Option<Vec<String>>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    module: Option<String>,
    #[serde(default)]
    experimental_decorators: bool,
    #[serde(default)]
    no_resolve: bool,
    #[serde(default)]
    check_js: bool,
    #[serde(default)]
    resolve_json_module: bool,
    #[serde(default)]
    no_unchecked_side_effect_imports: bool,
    #[serde(default)]
    no_implicit_override: bool,
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
        let mut config = Self {
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
    /// Cache for unified lib binder: (sorted lib names, unified `LibFile`)
    /// This avoids recreating the expensive merged binder on every request
    pub(crate) unified_lib_cache: Option<(Vec<String>, Arc<LibFile>)>,
    /// Number of checks completed
    pub(crate) checks_completed: u64,
    /// Response sequence counter (for tsserver protocol)
    pub(crate) response_seq: u64,
    /// Open files (for tsserver protocol)
    pub(crate) open_files: FxHashMap<String, String>,
    /// Completion preference: import module specifier ending (e.g. "js")
    pub(crate) completion_import_module_specifier_ending: Option<String>,
    /// Server mode
    pub(crate) _server_mode: ServerMode,
    /// Log configuration
    pub(crate) _log_config: LogConfig,
    /// Whether telemetry responses should be emitted.
    pub(crate) enable_telemetry: bool,
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
        info!("Using lib directory: {}", lib_dir.display());

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
            debug!("TSS_DEBUG detected: port {}", port);
        }
        if let Ok(port) = std::env::var("TSS_DEBUG_BRK") {
            debug!("TSS_DEBUG_BRK detected: port {} (break on startup)", port);
        }

        if log_config.level != LogLevel::Off {
            if let Some(ref file) = log_config.file {
                info!("Log file: {}", file.display());
            }
            info!("Log level: {:?}", log_config.level);
        }

        Ok(Self {
            lib_dir,
            tests_lib_dir,
            lib_cache: FxHashMap::default(),
            unified_lib_cache: None,
            checks_completed: 0,
            response_seq: 0,
            open_files: FxHashMap::default(),
            completion_import_module_specifier_ending: None,
            _server_mode: server_mode,
            _log_config: log_config,
            enable_telemetry: args.enable_telemetry,
        })
    }

    const fn next_seq(&mut self) -> u64 {
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
        let content = self
            .open_files
            .get(file_path)
            .cloned()
            .or_else(|| std::fs::read_to_string(file_path).ok())?;
        let mut parser = ParserState::new(file_path.to_string(), content.clone());
        let root = parser.parse_source_file();
        let arena = parser.into_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(&arena, root);
        Some((arena, binder, root, content))
    }

    /// Extract file path, line, and offset from tsserver request arguments.
    /// Returns (file, `line_1based`, `offset_1based`).
    fn extract_file_position(args: &serde_json::Value) -> Option<(String, u32, u32)> {
        let file = args.get("file")?.as_str()?.to_string();
        let line = args.get("line")?.as_u64()? as u32;
        let offset = args.get("offset")?.as_u64()? as u32;
        Some((file, line, offset))
    }

    /// Convert tsserver 1-based line/offset to 0-based LSP Position.
    pub(crate) const fn tsserver_to_lsp_position(line: u32, offset: u32) -> Position {
        Position::new(line.saturating_sub(1), offset.saturating_sub(1))
    }

    /// Convert LSP 0-based Position to tsserver 1-based {line, offset} JSON.
    fn lsp_to_tsserver_position(pos: Position) -> serde_json::Value {
        serde_json::json!({
            "line": pos.line + 1,
            "offset": pos.character + 1
        })
    }

    /// Convert a `DefinitionInfo` to a tsserver-compatible JSON value.
    fn definition_info_to_json(
        info: &tsz::lsp::definition::DefinitionInfo,
        file: &str,
    ) -> serde_json::Value {
        let out_file = if info.location.file_path.is_empty() {
            file.to_string()
        } else {
            info.location.file_path.clone()
        };
        let mut result = serde_json::json!({
            "file": out_file,
            "start": Self::lsp_to_tsserver_position(info.location.range.start),
            "end": Self::lsp_to_tsserver_position(info.location.range.end),
            "kind": info.kind,
            "name": info.name,
            "containerName": info.container_name,
            "containerKind": info.container_kind,
            "isLocal": info.is_local,
            "isAmbient": info.is_ambient,
            "unverified": false,
        });
        if info.kind == "alias" {
            result["failedAliasResolution"] = serde_json::json!(false);
        }
        if let Some(ref ctx) = info.context_span {
            result["contextStart"] = Self::lsp_to_tsserver_position(ctx.start);
            result["contextEnd"] = Self::lsp_to_tsserver_position(ctx.end);
        }
        result
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
            "brace" => self.handle_brace(seq, &request),
            "tszPerformance" | "performance" => self.handle_tsz_performance(seq, &request),
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
}

// =============================================================================
// Brace Matching Helpers
// =============================================================================

/// Build a boolean map indicating which byte positions are "in code"
/// (i.e., not inside a string literal or comment).
fn build_code_map(bytes: &[u8]) -> Vec<bool> {
    let len = bytes.len();
    let mut map = vec![true; len];
    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'/' if i + 1 < len => {
                if bytes[i + 1] == b'/' {
                    // Single-line comment
                    map[i] = false;
                    map[i + 1] = false;
                    i += 2;
                    while i < len && bytes[i] != b'\n' {
                        map[i] = false;
                        i += 1;
                    }
                } else if bytes[i + 1] == b'*' {
                    // Multi-line comment
                    map[i] = false;
                    map[i + 1] = false;
                    i += 2;
                    while i < len {
                        if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                            map[i] = false;
                            map[i + 1] = false;
                            i += 2;
                            break;
                        }
                        map[i] = false;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            b'"' | b'\'' => {
                let quote = bytes[i];
                map[i] = false;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        map[i] = false;
                        i += 1;
                        if i < len {
                            map[i] = false;
                            i += 1;
                        }
                    } else if bytes[i] == quote {
                        map[i] = false;
                        i += 1;
                        break;
                    } else if bytes[i] == b'\n' {
                        // Unterminated string at newline
                        break;
                    } else {
                        map[i] = false;
                        i += 1;
                    }
                }
            }
            b'`' => {
                // Template literal - mark everything inside as non-code
                // except for ${...} substitutions
                map[i] = false;
                i += 1;
                let mut depth = 0u32;
                while i < len {
                    if bytes[i] == b'\\' {
                        map[i] = false;
                        i += 1;
                        if i < len {
                            map[i] = false;
                            i += 1;
                        }
                    } else if bytes[i] == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
                        // Template substitution - these are code
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'{' && depth > 0 {
                        depth += 1;
                        i += 1;
                    } else if bytes[i] == b'}' && depth > 0 {
                        depth -= 1;
                        i += 1;
                    } else if bytes[i] == b'`' && depth == 0 {
                        map[i] = false;
                        i += 1;
                        break;
                    } else {
                        if depth == 0 {
                            map[i] = false;
                        }
                        i += 1;
                    }
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    map
}

/// Scan forward from `start` (exclusive) to find the matching closing brace.
/// Returns the byte offset of the matching close brace, or None.
fn scan_forward(
    bytes: &[u8],
    code_map: &[bool],
    start: usize,
    open: u8,
    close: u8,
) -> Option<usize> {
    let mut depth = 1i32;
    let mut i = start + 1;
    while i < bytes.len() {
        if code_map[i] {
            if bytes[i] == open {
                depth += 1;
            } else if bytes[i] == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

/// Scan backward from `start` (exclusive) to find the matching opening brace.
/// Returns the byte offset of the matching open brace, or None.
fn scan_backward(
    bytes: &[u8],
    code_map: &[bool],
    start: usize,
    close: u8,
    open: u8,
) -> Option<usize> {
    let mut depth = 1i32;
    let mut i = start;
    while i > 0 {
        i -= 1;
        if code_map[i] {
            if bytes[i] == close {
                depth += 1;
            } else if bytes[i] == open {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// Find matching angle bracket using AST-based analysis.
/// Returns the byte offset of the matching bracket, or None.
fn find_angle_bracket_match(arena: &NodeArena, source: &str, pos: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut pairs: Vec<(usize, usize)> = Vec::new();

    // Derive angle bracket positions from NodeList children.
    // The NodeList pos/end may be 0/0 (unset), but we can find the `<` and `>`
    // by looking at the first/last child nodes:
    //   `<` is at first_child.pos - 1
    //   `>` is at last_child.end - 1 (if parser includes `>` in range)
    //        or last_child.end (if parser excludes `>` from range)
    let check_list_nodes = |list: &Option<tsz::parser::base::NodeList>| -> Option<(usize, usize)> {
        let list = list.as_ref()?;
        if list.nodes.is_empty() {
            return None;
        }
        let first = arena.nodes.get(list.nodes.first()?.0 as usize)?;
        let last = arena.nodes.get(list.nodes.last()?.0 as usize)?;

        let open_pos = (first.pos as usize).checked_sub(1)?;
        if bytes.get(open_pos) != Some(&b'<') {
            return None;
        }

        // Try last_child.end - 1 first (parser includes `>` in range)
        let close_candidate1 = last.end as usize;
        if close_candidate1 > 0 && bytes.get(close_candidate1 - 1) == Some(&b'>') {
            return Some((open_pos, close_candidate1 - 1));
        }
        // Try last_child.end (parser excludes `>` from range)
        if bytes.get(close_candidate1) == Some(&b'>') {
            return Some((open_pos, close_candidate1));
        }
        None
    };

    // Collect from all data pools that have type_parameters or type_arguments
    for f in &arena.functions {
        if let Some(pair) = check_list_nodes(&f.type_parameters) {
            pairs.push(pair);
        }
    }
    for c in &arena.classes {
        if let Some(pair) = check_list_nodes(&c.type_parameters) {
            pairs.push(pair);
        }
    }
    for iface in &arena.interfaces {
        if let Some(pair) = check_list_nodes(&iface.type_parameters) {
            pairs.push(pair);
        }
    }
    for t in &arena.type_aliases {
        if let Some(pair) = check_list_nodes(&t.type_parameters) {
            pairs.push(pair);
        }
    }
    for c in &arena.call_exprs {
        if let Some(pair) = check_list_nodes(&c.type_arguments) {
            pairs.push(pair);
        }
    }
    for t in &arena.type_refs {
        if let Some(pair) = check_list_nodes(&t.type_arguments) {
            pairs.push(pair);
        }
    }
    for s in &arena.signatures {
        if let Some(pair) = check_list_nodes(&s.type_parameters) {
            pairs.push(pair);
        }
    }
    for m in &arena.method_decls {
        if let Some(pair) = check_list_nodes(&m.type_parameters) {
            pairs.push(pair);
        }
    }
    for c in &arena.constructors {
        if let Some(pair) = check_list_nodes(&c.type_parameters) {
            pairs.push(pair);
        }
    }
    for ft in &arena.function_types {
        if let Some(pair) = check_list_nodes(&ft.type_parameters) {
            pairs.push(pair);
        }
    }
    for e in &arena.expr_with_type_args {
        if let Some(pair) = check_list_nodes(&e.type_arguments) {
            pairs.push(pair);
        }
    }

    // Type assertions: <type>expr
    for node in &arena.nodes {
        if node.kind == tsz::parser::syntax_kind_ext::TYPE_ASSERTION
            && let Some(ta) = arena.type_assertions.get(node.data_index as usize)
        {
            let open_pos = node.pos as usize;
            if bytes.get(open_pos) != Some(&b'<') {
                continue;
            }
            if let Some(type_node) = arena.nodes.get(ta.type_node.0 as usize) {
                // `>` might be at type_node.end - 1 or type_node.end
                let end = type_node.end as usize;
                if end > 0 && bytes.get(end - 1) == Some(&b'>') {
                    pairs.push((open_pos, end - 1));
                } else if bytes.get(end) == Some(&b'>') {
                    pairs.push((open_pos, end));
                }
            }
        }
    }

    // Search for the position in collected pairs
    for (open, close) in pairs {
        if pos == open {
            return Some(close);
        } else if pos == close {
            return Some(open);
        }
    }

    None
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
    // Initialize tracing (always stderr so it doesn't interfere with protocol).
    // Supports TSZ_LOG_FORMAT=tree|json|text (see src/tracing_config.rs).
    tsz_cli::tracing_config::init_tracing();

    let args = ServerArgs::parse();
    let mut server = Server::new(&args).context("failed to initialize server")?;

    info!("tsz-server ready (protocol: {:?})", args.protocol);

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
                    message: Some(format!("invalid request: {e}")),
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
                    error: format!("invalid request: {e}"),
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
#[path = "tests.rs"]
mod tests;
