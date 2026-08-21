//! Deterministic program construction and phase coordination.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::bind::{BoundFile, Meaning, bind_source};
use crate::config::{CompilerOptionKey, ProjectProvenance, ResolvedProject};
use crate::diagnostics::{Diagnostic, DiagnosticCategory, sort_and_deduplicate};
use crate::emit::emit_file_with_plan;
use crate::emit_paths::EmitPlan;
use crate::semantics::{CheckResult, check_program};
use crate::source::{DeclId, FileId, SourceText};
use crate::standard_library::{StandardLibraryDeclaration, StandardLibraryEnvironment};
use crate::syntax::{SourceUnit, parse_source};

mod import_aliases;

#[derive(Debug, Clone)]
pub struct SourceInput {
    /// Logical path used in diagnostics, emit, and service responses.
    pub path: PathBuf,
    /// Host path used to read and resolve this source.
    pub host_path: PathBuf,
    pub text: Arc<str>,
}

impl SourceInput {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, text: impl Into<Arc<str>>) -> Self {
        let path = path.into();
        Self {
            host_path: path.clone(),
            path,
            text: text.into(),
        }
    }

    #[must_use]
    pub fn with_host_path(
        path: impl Into<PathBuf>,
        host_path: impl Into<PathBuf>,
        text: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            path: path.into(),
            host_path: host_path.into(),
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompilerOptions {
    pub strict: bool,
    /// `None` inherits `strict`; `Some(false)` explicitly opts out.
    pub no_implicit_any: Option<bool>,
    pub no_lib: bool,
    pub lib: Option<Vec<String>>,
    pub allow_js: bool,
    pub no_check: bool,
    pub no_emit: bool,
    pub no_emit_on_error: bool,
    pub declaration: bool,
    pub declaration_map: bool,
    pub source_map: bool,
    pub inline_source_map: bool,
    pub remove_comments: bool,
    pub target: String,
    pub module: String,
    pub root_dir: Option<PathBuf>,
    pub out_dir: Option<PathBuf>,
    pub declaration_dir: Option<PathBuf>,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            strict: true,
            no_implicit_any: None,
            no_lib: false,
            lib: None,
            allow_js: false,
            no_check: false,
            no_emit: false,
            no_emit_on_error: false,
            declaration: false,
            declaration_map: false,
            source_map: false,
            inline_source_map: false,
            remove_comments: false,
            target: "es2025".to_string(),
            module: "preserve".to_string(),
            root_dir: None,
            out_dir: None,
            declaration_dir: None,
        }
    }
}

impl CompilerOptions {
    #[must_use]
    pub const fn effective_no_implicit_any(&self) -> bool {
        match self.no_implicit_any {
            Some(value) => value,
            None => self.strict,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProgramFile {
    pub source: SourceText,
    pub syntax: SourceUnit,
    pub bindings: BoundFile,
}

impl ProgramFile {
    /// Whether this source owns a module-local root scope. `.mts`/`.cts`
    /// sources are modules by path even without authored import/export syntax.
    #[must_use]
    pub fn is_external_module(&self) -> bool {
        self.syntax.is_external_module()
            || self.source.path.extension().is_some_and(|extension| {
                let extension = extension.to_string_lossy();
                extension.eq_ignore_ascii_case("mts") || extension.eq_ignore_ascii_case("cts")
            })
    }
}

#[derive(Debug)]
pub struct Program {
    /// Program traversal order, independent from path-sorted `FileId` storage.
    ///
    /// TypeScript preserves configured root/dependency order for binding and
    /// checking. Keeping that order explicit lets `FileId` remain a stable
    /// path identity without silently changing observable program order.
    pub source_order: Vec<FileId>,
    pub files: Vec<ProgramFile>,
    pub global_values: BTreeMap<String, Vec<DeclId>>,
    pub global_types: BTreeMap<String, Vec<DeclId>>,
    pub standard_library: StandardLibraryEnvironment,
    import_aliases: import_aliases::ImportAliases,
}

impl Program {
    #[must_use]
    pub fn source(&self, id: FileId) -> Option<&SourceText> {
        self.files.get(id.0 as usize).map(|file| &file.source)
    }

    #[must_use]
    pub fn file(&self, id: FileId) -> Option<&ProgramFile> {
        self.files.get(id.0 as usize)
    }

    #[must_use]
    pub fn resolve_global(&self, name: &str, meaning: Meaning) -> Option<DeclId> {
        let table = match meaning {
            Meaning::Value => &self.global_values,
            Meaning::Type => &self.global_types,
        };
        table
            .get(name)
            .and_then(|ids| ids.first().copied())
            .or_else(|| self.standard_library.resolve(name, meaning))
    }

    #[must_use]
    pub fn standard_library_declaration(&self, id: DeclId) -> Option<&StandardLibraryDeclaration> {
        self.standard_library.declaration(id)
    }

    fn missing_essential_global_types(&self) -> Vec<&'static str> {
        StandardLibraryEnvironment::essential_type_names()
            .iter()
            .copied()
            .filter(|name| self.resolve_global(name, Meaning::Type).is_none())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmittedFile {
    pub path: PathBuf,
    pub text: String,
    pub declaration: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompileStats {
    /// Aggregate verdict for every semantic query required by this checked
    /// compile. Project/performance consumers may claim compatibility only
    /// when this is [`SemanticCompletion::Complete`].
    pub semantic_completion: SemanticCompletion,
    /// Backwards-compatible alias for `source_files`.
    pub files: usize,
    pub root_files: usize,
    pub source_files: usize,
    /// Ordered host paths selected as roots, excluding synthetic libraries.
    pub root_file_paths: Vec<String>,
    /// Ordered host paths admitted to the authored source graph.
    pub source_file_paths: Vec<String>,
    pub project_configs: usize,
    pub project_references: usize,
    pub lines: usize,
    pub identifiers: usize,
    pub symbols: usize,
    pub types: usize,
    pub parse_time_ms: f64,
    pub bind_time_ms: f64,
    pub check_time_ms: f64,
    pub emit_time_ms: f64,
    pub total_time_ms: f64,
}

#[derive(Debug)]
pub struct CompileOutput {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
    pub emitted_files: Vec<EmittedFile>,
    pub stats: CompileStats,
    pub semantic_completion: SemanticCompletion,
    pub exit_status: CompileExitStatus,
}

/// Whether every required semantic operation reached a definitive result.
///
/// Dominance is deterministic and ordered from least to most severe:
/// `Complete < Deferred < Cycle < Limit`. A symbolic type that later resolves
/// does not affect this verdict; only an incomplete result escaping at a
/// user-visible checking boundary is aggregated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemanticCompletion {
    #[default]
    Complete,
    Deferred,
    Cycle,
    Limit,
}

impl SemanticCompletion {
    #[must_use]
    pub const fn combine(self, other: Self) -> Self {
        if self as u8 >= other as u8 {
            self
        } else {
            other
        }
    }

    #[must_use]
    pub const fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Deferred => "deferred",
            Self::Cycle => "cycle",
            Self::Limit => "limit",
        }
    }
}

/// TypeScript's process-level compile result.
///
/// "Outputs skipped" is not equivalent to an empty emitted-file list: a
/// collision can skip one product while safe products from the same program
/// are still generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileExitStatus {
    Success,
    DiagnosticsPresentOutputsSkipped,
    DiagnosticsPresentOutputsGenerated,
    /// Checking could not decide at least one required semantic operation.
    /// This is a compiler capability nonclaim, not a TypeScript diagnostic.
    SemanticIncomplete,
}

impl CompileExitStatus {
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::DiagnosticsPresentOutputsSkipped => 1,
            Self::DiagnosticsPresentOutputsGenerated => 2,
            Self::SemanticIncomplete => 3,
        }
    }
}

#[derive(Debug, Default)]
pub struct Compiler;

impl Compiler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn compile(&self, inputs: Vec<SourceInput>, options: &CompilerOptions) -> CompileOutput {
        let root_file_paths = inputs
            .iter()
            .map(|input| normalized_path(&input.host_path))
            .collect::<Vec<_>>();
        let source_files = root_file_paths.len();
        self.compile_inputs(
            inputs,
            options,
            Vec::new(),
            ProjectProvenance::default(),
            ProjectStats {
                root_files: source_files,
                root_file_paths,
                project_configs: 0,
                project_references: 0,
            },
        )
    }

    /// Compile a host-resolved project while preserving configuration
    /// diagnostics and graph metrics in the ordinary compiler result.
    pub fn compile_resolved(
        &self,
        resolved: ResolvedProject,
        options: &CompilerOptions,
    ) -> CompileOutput {
        let stats = ProjectStats {
            root_files: resolved.root_file_count(),
            root_file_paths: resolved
                .root_files
                .iter()
                .map(|path| normalized_path(path))
                .collect(),
            project_configs: resolved.project_config_count(),
            project_references: resolved.project_reference_count(),
        };
        self.compile_inputs(
            resolved.inputs,
            options,
            resolved.diagnostics,
            resolved.provenance,
            stats,
        )
    }

    fn compile_inputs(
        &self,
        mut inputs: Vec<SourceInput>,
        options: &CompilerOptions,
        mut diagnostics: Vec<Diagnostic>,
        provenance: ProjectProvenance,
        project_stats: ProjectStats,
    ) -> CompileOutput {
        let total_start = Instant::now();
        let mut source_order_keys = Vec::with_capacity(inputs.len());
        let mut seen_source_keys = BTreeSet::new();
        for input in &inputs {
            let key = normalized_path(&input.host_path);
            if seen_source_keys.insert(key.clone()) {
                source_order_keys.push(key);
            }
        }
        inputs.sort_by_cached_key(|input| {
            (
                normalized_path(&input.host_path),
                normalized_path(&input.path),
            )
        });
        inputs.dedup_by(|left, right| {
            normalized_path(&left.host_path) == normalized_path(&right.host_path)
        });

        let sources: Vec<SourceText> = inputs
            .into_iter()
            .enumerate()
            .map(|(ordinal, input)| {
                SourceText::new_with_host_path(
                    FileId(ordinal as u32),
                    input.path,
                    input.host_path,
                    input.text,
                )
            })
            .collect();

        let source_ids = sources
            .iter()
            .map(|source| (normalized_path(&source.host_path), source.id))
            .collect::<BTreeMap<_, _>>();
        let source_order = source_order_keys
            .iter()
            .filter_map(|key| source_ids.get(key).copied())
            .collect::<Vec<_>>();

        let jobs: Vec<ParseBindJob> = sources
            .into_par_iter()
            .map(|source| {
                let parse_start = Instant::now();
                let parsed = parse_source(&source);
                let parse_time = parse_start.elapsed();
                let bind_start = Instant::now();
                let bindings = bind_source(source.id, &parsed.unit);
                let bind_time = bind_start.elapsed();
                ParseBindJob {
                    file: ProgramFile {
                        source,
                        syntax: parsed.unit,
                        bindings,
                    },
                    diagnostics: parsed.diagnostics,
                    parse_time,
                    bind_time,
                }
            })
            .collect();

        let mut files = Vec::with_capacity(jobs.len());
        let mut parse_time = Duration::ZERO;
        let mut bind_time = Duration::ZERO;
        for job in jobs {
            diagnostics.extend(job.diagnostics);
            parse_time += job.parse_time;
            bind_time += job.bind_time;
            files.push(job.file);
        }
        let option_diagnostics = compiler_option_diagnostics(options, &provenance);
        let has_fatal_option_error = !option_diagnostics.is_empty()
            && provenance
                .option_origin(CompilerOptionKey::Target)
                .is_none();
        diagnostics.extend(option_diagnostics);
        files.sort_by_key(|file| file.source.id);
        let program = build_program(files, source_order, options);
        let missing_essential_types = if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.category == DiagnosticCategory::Error)
        {
            Vec::new()
        } else {
            program.missing_essential_global_types()
        };
        let has_missing_essential_types = !missing_essential_types.is_empty();

        let check_start = Instant::now();
        let CheckResult {
            diagnostics: semantic_diagnostics,
            type_count,
            mut semantic_completion,
        } = if options.no_check || has_missing_essential_types || has_fatal_option_error {
            CheckResult {
                diagnostics: Vec::new(),
                type_count: 0,
                semantic_completion: SemanticCompletion::Complete,
            }
        } else {
            check_program(&program, options)
        };
        let check_time = check_start.elapsed();
        diagnostics.extend(
            missing_essential_types
                .into_iter()
                .map(|name| Diagnostic::global(format!("Cannot find global type '{name}'."), 2318)),
        );
        diagnostics.extend(semantic_diagnostics);
        let emit_start = Instant::now();
        let emit_plan = if has_fatal_option_error {
            EmitPlan::empty(program.files.len())
        } else {
            EmitPlan::for_program(&program.files, options, &provenance)
        };
        if emit_plan.has_incomplete_products() {
            semantic_completion = semantic_completion.combine(SemanticCompletion::Deferred);
        }
        diagnostics.extend(emit_plan.diagnostics().iter().cloned());
        // TypeScript's observable diagnostic order is a deterministic total
        // order, not root discovery order. `FileId` is assigned from the
        // canonical path-sorted source set, so the ordinary merge remains
        // identical when callers reverse their root list.
        sort_and_deduplicate(&mut diagnostics);

        let has_errors = diagnostics
            .iter()
            .any(|diagnostic| diagnostic.category == DiagnosticCategory::Error);
        let emit_suppressed =
            options.no_emit || has_fatal_option_error || (has_errors && options.no_emit_on_error);
        let mut emitted_files = if emit_suppressed {
            Vec::new()
        } else {
            program
                .files
                .par_iter()
                .flat_map_iter(|file| {
                    emit_file_with_plan(file, options, emit_plan.for_file(file.source.id))
                })
                .collect()
        };
        emitted_files.sort_by(|left, right| left.path.cmp(&right.path));
        let planned_declarations = program
            .files
            .iter()
            .filter(|file| emit_plan.for_file(file.source.id).declaration.is_some())
            .count();
        let planned_javascript = program
            .files
            .iter()
            .filter(|file| emit_plan.for_file(file.source.id).javascript.is_some())
            .count();
        if !emit_suppressed
            && (emitted_files.iter().filter(|file| file.declaration).count() < planned_declarations
                || emitted_files
                    .iter()
                    .filter(|file| !file.declaration)
                    .count()
                    < planned_javascript)
        {
            semantic_completion = semantic_completion.combine(SemanticCompletion::Deferred);
        }
        let emit_time = emit_start.elapsed();

        let lines = program
            .files
            .iter()
            .map(|file| file.source.text.lines().count())
            .sum();
        let symbols = program
            .files
            .iter()
            .map(|file| file.bindings.declarations.len())
            .sum();
        let identifiers = symbols;
        let stats = CompileStats {
            semantic_completion,
            files: program.files.len(),
            root_files: project_stats.root_files,
            source_files: program.files.len(),
            root_file_paths: project_stats.root_file_paths,
            source_file_paths: program
                .source_order
                .iter()
                .map(|id| normalized_path(&program.files[id.0 as usize].source.host_path))
                .collect(),
            project_configs: project_stats.project_configs,
            project_references: project_stats.project_references,
            lines,
            identifiers,
            symbols,
            types: type_count,
            parse_time_ms: milliseconds(parse_time),
            bind_time_ms: milliseconds(bind_time),
            check_time_ms: milliseconds(check_time),
            emit_time_ms: milliseconds(emit_time),
            total_time_ms: milliseconds(total_start.elapsed()),
        };

        let exit_status = if !semantic_completion.is_complete() {
            CompileExitStatus::SemanticIncomplete
        } else if !has_errors {
            CompileExitStatus::Success
        } else if program.files.is_empty()
            && (project_stats.root_files > 0 || project_stats.project_configs > 0)
        {
            CompileExitStatus::DiagnosticsPresentOutputsGenerated
        } else if options.no_emit
            || options.no_emit_on_error
            || has_fatal_option_error
            || emit_plan.has_blocked_products()
        {
            CompileExitStatus::DiagnosticsPresentOutputsSkipped
        } else {
            CompileExitStatus::DiagnosticsPresentOutputsGenerated
        };

        CompileOutput {
            program,
            diagnostics,
            emitted_files,
            stats,
            semantic_completion,
            exit_status,
        }
    }
}

#[derive(Debug, Clone)]
struct ProjectStats {
    root_files: usize,
    root_file_paths: Vec<String>,
    project_configs: usize,
    project_references: usize,
}

fn compiler_option_diagnostics(
    options: &CompilerOptions,
    provenance: &ProjectProvenance,
) -> Vec<Diagnostic> {
    let target = options.target.trim().to_ascii_lowercase();
    let diagnostic = match target.as_str() {
        "es3" => Some((
            "Option 'target=ES3' has been removed. Please remove it from your configuration."
                .to_string(),
            5108,
        )),
        "es5" => Some((
            "Option 'target=ES5' has been removed. Please remove it from your configuration."
                .to_string(),
            5108,
        )),
        "es6" | "es2015" | "es2016" | "es2017" | "es2018" | "es2019" | "es2020" | "es2021"
        | "es2022" | "es2023" | "es2024" | "es2025" | "esnext" => None,
        _ => Some((
            concat!(
                "Argument for '--target' option must be: 'es6', 'es2015', 'es2016', ",
                "'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', ",
                "'es2023', 'es2024', 'es2025', 'esnext'."
            )
            .to_string(),
            6046,
        )),
    };
    diagnostic
        .map(|(message, code)| {
            if let Some(origin) = provenance.option_origin(CompilerOptionKey::Target) {
                origin.diagnostic_at_value(message, code)
            } else {
                Diagnostic::global(message, code)
            }
        })
        .into_iter()
        .collect()
}

struct ParseBindJob {
    file: ProgramFile,
    diagnostics: Vec<Diagnostic>,
    parse_time: Duration,
    bind_time: Duration,
}

fn build_program(
    files: Vec<ProgramFile>,
    source_order: Vec<FileId>,
    options: &CompilerOptions,
) -> Program {
    let import_aliases = import_aliases::ImportAliases::build(&files, options.allow_js);
    let mut global_values: BTreeMap<String, Vec<DeclId>> = BTreeMap::new();
    let mut global_types: BTreeMap<String, Vec<DeclId>> = BTreeMap::new();
    for file_id in &source_order {
        let file = &files[file_id.0 as usize];
        if file.is_external_module() {
            // External-module roots are file-local binding scopes. They may
            // fall back to the script global scope, but must never contribute
            // their own declarations to that scope.
            continue;
        }
        let root = &file.bindings.scopes[0];
        for ids in root.names.values() {
            for id in ids {
                let Some(declaration) = file.bindings.declaration(*id) else {
                    continue;
                };
                let table = match declaration.meaning {
                    Meaning::Value => &mut global_values,
                    Meaning::Type => &mut global_types,
                };
                table.entry(declaration.name.clone()).or_default().push(*id);
            }
        }
    }
    Program {
        source_order,
        files,
        global_values,
        global_types,
        standard_library: StandardLibraryEnvironment::from_options(options),
        import_aliases,
    }
}

fn normalized_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
