//! Deterministic program construction and phase coordination.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::bind::{BoundFile, Meaning, bind_source};
use crate::diagnostics::{Diagnostic, DiagnosticCategory, sort_and_deduplicate};
use crate::emit::emit_file;
use crate::semantics::{CheckResult, check_program};
use crate::source::{DeclId, FileId, SourceText};
use crate::standard_library::{StandardLibraryDeclaration, StandardLibraryEnvironment};
use crate::syntax::{SourceUnit, parse_source};

#[derive(Debug, Clone)]
pub struct SourceInput {
    pub path: PathBuf,
    pub text: Arc<str>,
}

impl SourceInput {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, text: impl Into<Arc<str>>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompilerOptions {
    pub strict: bool,
    pub no_implicit_any: bool,
    pub no_lib: bool,
    pub lib: Option<Vec<String>>,
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
    pub out_dir: Option<PathBuf>,
    pub declaration_dir: Option<PathBuf>,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            strict: false,
            no_implicit_any: false,
            no_lib: false,
            lib: None,
            no_check: false,
            no_emit: false,
            no_emit_on_error: false,
            declaration: false,
            declaration_map: false,
            source_map: false,
            inline_source_map: false,
            remove_comments: false,
            target: "es2025".to_string(),
            module: "commonjs".to_string(),
            out_dir: None,
            declaration_dir: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProgramFile {
    pub source: SourceText,
    pub syntax: SourceUnit,
    pub bindings: BoundFile,
}

#[derive(Debug)]
pub struct Program {
    pub files: Vec<ProgramFile>,
    pub global_values: BTreeMap<String, Vec<DeclId>>,
    pub global_types: BTreeMap<String, Vec<DeclId>>,
    pub standard_library: StandardLibraryEnvironment,
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
    pub files: usize,
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
}

#[derive(Debug, Default)]
pub struct Compiler;

impl Compiler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn compile(
        &self,
        mut inputs: Vec<SourceInput>,
        options: &CompilerOptions,
    ) -> CompileOutput {
        let total_start = Instant::now();
        inputs.sort_by_cached_key(|input| normalized_path(&input.path));
        inputs.dedup_by(|left, right| normalized_path(&left.path) == normalized_path(&right.path));

        let sources: Vec<SourceText> = inputs
            .into_iter()
            .enumerate()
            .map(|(ordinal, input)| SourceText::new(FileId(ordinal as u32), input.path, input.text))
            .collect();

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

        let mut diagnostics = Vec::new();
        let mut files = Vec::with_capacity(jobs.len());
        let mut parse_time = Duration::ZERO;
        let mut bind_time = Duration::ZERO;
        for job in jobs {
            diagnostics.extend(job.diagnostics);
            parse_time += job.parse_time;
            bind_time += job.bind_time;
            files.push(job.file);
        }
        let option_diagnostics = compiler_option_diagnostics(options);
        let has_fatal_option_error = !option_diagnostics.is_empty();
        diagnostics.extend(option_diagnostics);
        files.sort_by_key(|file| file.source.id);
        let program = build_program(files, options);
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
        } = if options.no_check || has_missing_essential_types || has_fatal_option_error {
            CheckResult {
                diagnostics: Vec::new(),
                type_count: 0,
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
        sort_and_deduplicate(&mut diagnostics);

        let has_errors = diagnostics
            .iter()
            .any(|diagnostic| diagnostic.category == DiagnosticCategory::Error);
        let emit_start = Instant::now();
        let mut emitted_files = if options.no_emit
            || has_fatal_option_error
            || (has_errors && options.no_emit_on_error)
        {
            Vec::new()
        } else {
            program
                .files
                .par_iter()
                .flat_map_iter(|file| emit_file(file, options))
                .collect()
        };
        emitted_files.sort_by(|left, right| left.path.cmp(&right.path));
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
            files: program.files.len(),
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

        CompileOutput {
            program,
            diagnostics,
            emitted_files,
            stats,
        }
    }
}

fn compiler_option_diagnostics(options: &CompilerOptions) -> Vec<Diagnostic> {
    let target = options.target.trim().to_ascii_lowercase();
    let diagnostic = match target.as_str() {
        "es3" => Some(Diagnostic::global(
            "Option 'target=ES3' has been removed. Please remove it from your configuration."
                .to_string(),
            5108,
        )),
        "es5" => Some(Diagnostic::global(
            "Option 'target=ES5' has been removed. Please remove it from your configuration."
                .to_string(),
            5108,
        )),
        "es6" | "es2015" | "es2016" | "es2017" | "es2018" | "es2019" | "es2020" | "es2021"
        | "es2022" | "es2023" | "es2024" | "es2025" | "esnext" => None,
        _ => Some(Diagnostic::global(
            concat!(
                "Argument for '--target' option must be: 'es6', 'es2015', 'es2016', ",
                "'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', ",
                "'es2023', 'es2024', 'es2025', 'esnext'."
            )
            .to_string(),
            6046,
        )),
    };
    diagnostic.into_iter().collect()
}

struct ParseBindJob {
    file: ProgramFile,
    diagnostics: Vec<Diagnostic>,
    parse_time: Duration,
    bind_time: Duration,
}

fn build_program(files: Vec<ProgramFile>, options: &CompilerOptions) -> Program {
    let mut global_values: BTreeMap<String, Vec<DeclId>> = BTreeMap::new();
    let mut global_types: BTreeMap<String, Vec<DeclId>> = BTreeMap::new();
    for file in &files {
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
        files,
        global_values,
        global_types,
        standard_library: StandardLibraryEnvironment::from_options(options),
    }
}

fn normalized_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
