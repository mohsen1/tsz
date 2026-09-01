//! Deterministic program construction and phase coordination.
use crate::bind::{BoundFile, DeclarationKind, Meaning, TypeMemberSymbol, bind_source_with_kind};
use crate::config::{
    CompilerOptionKey, ProjectProvenance, ProjectResolution, ResolvedProject, TargetValueOutcome,
    classify_target_value,
};
use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticPhase, sort_and_deduplicate,
    sort_and_deduplicate_by_path, sort_and_deduplicate_for_cli,
};
use crate::emit::emit_file_with_plan;
use crate::emit_paths::EmitPlan;
use crate::semantics::{CheckResult, check_program, summarize_program};
use crate::source::{DeclId, FileId, SourceText, display_path};
use crate::standard_library::{StandardLibraryDeclaration, StandardLibraryEnvironment};
use crate::syntax::{SourceUnit, StatementKind, parse_source};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
mod capabilities;
mod diagnostic_products;
mod display_summary;
mod import_aliases;
mod javascript_assignments;
pub(crate) use capabilities::{
    CapabilityAnalysis, CapabilityContext, CapabilityScope, CapabilityTarget,
    is_declaration_source, is_effective_commonjs,
};
pub(crate) use display_summary::{
    DeclarationDisplayParts, DeclarationDisplaySummaries, DeclarationDisplaySummary,
    DefaultExportDeclaration, RenderedParameter, RenderedParameters, RenderedType,
};
pub(crate) use javascript_assignments::{JavaScriptAssignmentDisposition, JavaScriptAssignments};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeferredOptionEffect {
    SemanticTypes,
    JavaScript,
    Declaration,
    Emit,
    ImportHelpers,
    StrictEmit,
    DecoratorMetadata,
    Jsx,
    All,
}
macro_rules! deferred_compiler_options {
    ($($variant:ident => ($name:literal, $value_kind:ident, $effect:ident, $jsx:literal, $syntactic:literal)),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
        pub enum DeferredCompilerOption { $($variant),+ }
        impl DeferredCompilerOption {
            pub(crate) const ALL: &'static [Self] = &[$(Self::$variant),+];
            #[must_use]
            pub fn from_cli_name(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|option| option.json_name().eq_ignore_ascii_case(name))
            }
            #[must_use]
            pub const fn takes_value(self) -> bool { !self.is_boolean() }
            pub(crate) const fn is_boolean(self) -> bool {
                matches!(self.value_kind(), DeferredOptionValueKind::Boolean)
            }
            pub(crate) const fn is_path(self) -> bool {
                matches!(self.value_kind(), DeferredOptionValueKind::Path)
            }
            pub(crate) const fn is_jsx(self) -> bool { match self { $(Self::$variant => $jsx),+ } }
            pub(crate) const fn affects_syntactic_diagnostics(self) -> bool {
                match self { $(Self::$variant => $syntactic),+ }
            }
            pub(crate) const fn effect(self) -> DeferredOptionEffect {
                match self { $(Self::$variant => DeferredOptionEffect::$effect),+ }
            }
            pub(crate) const fn json_name(self) -> &'static str {
                match self { $(Self::$variant => $name),+ }
            }
            const fn value_kind(self) -> DeferredOptionValueKind {
                match self { $(Self::$variant => DeferredOptionValueKind::$value_kind),+ }
            }
        }
    };
}
#[derive(Clone, Copy)]
enum DeferredOptionValueKind {
    Boolean,
    String,
    Path,
}
deferred_compiler_options! {
    AlwaysStrict => ("alwaysStrict", Boolean, StrictEmit, false, true),
    NoImplicitThis => ("noImplicitThis", Boolean, SemanticTypes, false, false),
    StrictBindCallApply => ("strictBindCallApply", Boolean, SemanticTypes, false, false),
    StrictFunctionTypes => ("strictFunctionTypes", Boolean, SemanticTypes, false, false),
    UseUnknownInCatchVariables => ("useUnknownInCatchVariables", Boolean, SemanticTypes, false, false),
    DownlevelIteration => ("downlevelIteration", Boolean, JavaScript, false, false),
    NoEmitHelpers => ("noEmitHelpers", Boolean, JavaScript, false, false),
    ImportHelpers => ("importHelpers", Boolean, ImportHelpers, false, false),
    EsModuleInterop => ("esModuleInterop", Boolean, All, false, false),
    ExperimentalDecorators => ("experimentalDecorators", Boolean, All, false, true),
    EmitDecoratorMetadata => ("emitDecoratorMetadata", Boolean, DecoratorMetadata, false, false),
    ExactOptionalPropertyTypes => ("exactOptionalPropertyTypes", Boolean, SemanticTypes, false, false),
    PreserveConstEnums => ("preserveConstEnums", Boolean, JavaScript, false, false),
    VerbatimModuleSyntax => ("verbatimModuleSyntax", Boolean, All, false, false),
    RewriteRelativeImportExtensions => ("rewriteRelativeImportExtensions", Boolean, Emit, false, false),
    IsolatedModules => ("isolatedModules", Boolean, All, false, false),
    StripInternal => ("stripInternal", Boolean, Declaration, false, false),
    Jsx => ("jsx", String, Jsx, true, false),
    JsxFactory => ("jsxFactory", String, Jsx, true, false),
    JsxFragmentFactory => ("jsxFragmentFactory", String, Jsx, true, false),
    JsxImportSource => ("jsxImportSource", String, Jsx, true, false),
    ModuleResolution => ("moduleResolution", String, All, false, false),
    ModuleDetection => ("moduleDetection", String, All, false, false),
    BaseUrl => ("baseUrl", Path, All, false, false),
    OutFile => ("outFile", Path, Emit, false, false),
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeferredCompilerOptionValue {
    Boolean(bool),
    String(String),
    Path(PathBuf),
}
#[derive(Debug, Clone)]
pub struct SourceInput {
    pub path: PathBuf,
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
macro_rules! compiler_options {
    ($($(#[$meta:meta])* $field:ident: $ty:ty = $default:expr),+ $(,)?) => {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[serde(default)]
        pub struct CompilerOptions { $($(#[$meta])* pub $field: $ty),+ }
        impl Default for CompilerOptions {
            fn default() -> Self { Self { $($field: $default),+ } }
        }
    };
}
compiler_options! {
    strict: bool = true,
    /// `None` inherits `strict`; `Some(false)` explicitly opts out.
    strict_null_checks: Option<bool> = None,
    /// `None` inherits `strict`; `Some(false)` explicitly opts out.
    strict_property_initialization: Option<bool> = None,
    /// `None` inherits `strict`; `Some(false)` explicitly opts out.
    no_implicit_any: Option<bool> = None,
    no_unused_locals: bool = false,
    no_unused_parameters: bool = false,
    no_lib: bool = false,
    lib: Option<Vec<String>> = None,
    allow_js: bool = false,
    check_js: Option<bool> = None,
    no_check: bool = false,
    skip_lib_check: bool = false,
    no_emit: bool = false,
    no_emit_on_error: bool = false,
    declaration: bool = false,
    declaration_map: bool = false,
    source_map: bool = false,
    inline_source_map: bool = false,
    remove_comments: bool = false,
    /// `None` uses the target-dependent TypeScript default.
    use_define_for_class_fields: Option<bool> = None,
    target: String = "es2025".to_string(),
    module: String = "preserve".to_string(),
    root_dir: Option<PathBuf> = None,
    out_dir: Option<PathBuf> = None,
    declaration_dir: Option<PathBuf> = None,
    deferred_options: BTreeMap<DeferredCompilerOption, DeferredCompilerOptionValue> = BTreeMap::new(),
}
impl CompilerOptions {
    #[must_use]
    pub const fn effective_strict_null_checks(&self) -> bool {
        matches!(self.strict_null_checks, Some(true))
            || self.strict_null_checks.is_none() && self.strict
    }
    #[must_use]
    pub const fn effective_strict_property_initialization(&self) -> bool {
        matches!(self.strict_property_initialization, Some(true))
            || self.strict_property_initialization.is_none() && self.strict
    }
    #[must_use]
    pub const fn effective_no_implicit_any(&self) -> bool {
        matches!(self.no_implicit_any, Some(true)) || self.no_implicit_any.is_none() && self.strict
    }
}
#[derive(Debug, Clone)]
pub struct ProgramFile {
    pub source: SourceText,
    pub syntax: SourceUnit,
    pub bindings: BoundFile,
}
impl ProgramFile {
    /// Whether this source owns a module-local root scope, including module-format extensions.
    #[must_use]
    pub fn is_external_module(&self) -> bool {
        self.syntax.is_external_module() || self.has_source_extension(&["mts", "cts", "mjs", "cjs"])
    }
    /// Whether `.mjs`/`.cjs` runtime markers or declaration elision need unmodeled lowering.
    #[must_use]
    pub(crate) fn has_unmodeled_javascript_module_products(&self) -> bool {
        self.has_source_extension(&["mjs", "cjs"])
    }
    fn has_source_extension(&self, expected: &[&str]) -> bool {
        self.source.path.extension().is_some_and(|extension| {
            let extension = extension.to_string_lossy();
            expected
                .iter()
                .any(|expected| extension.eq_ignore_ascii_case(expected))
        })
    }
}
#[derive(Debug)]
pub struct Program {
    pub source_order: Vec<FileId>,
    pub files: Vec<ProgramFile>,
    pub global_values: BTreeMap<String, Vec<DeclId>>,
    pub global_types: BTreeMap<String, Vec<DeclId>>,
    pub standard_library: StandardLibraryEnvironment,
    pub(crate) standard_library_type_alias_collisions: Vec<(DeclId, DeclId)>,
    pub(crate) javascript_assignments: JavaScriptAssignments,
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
    pub(crate) fn standard_library_type_has_authored_declarations(&self, owner: DeclId) -> bool {
        self.standard_library
            .declaration(owner)
            .is_none_or(|owner| self.resolve_global(&owner.name, Meaning::Type) != Some(owner.id))
    }
    pub(crate) fn standard_library_type_has_authored_member(
        &self,
        owner: DeclId,
        member_name: &str,
    ) -> bool {
        let Some(owner_name) = self
            .standard_library
            .declaration(owner)
            .filter(|declaration| declaration.meaning == Meaning::Type)
            .map(|declaration| declaration.name.as_str())
        else {
            return true;
        };
        self.global_types
            .get(owner_name)
            .is_some_and(|declarations| {
                declarations.iter().copied().any(|declaration| {
                    self.file(declaration.file)
                        .and_then(|file| {
                            let bound = file.bindings.declaration(declaration)?;
                            let interface =
                                file.syntax.statements.iter().find_map(|statement| {
                                    (bound.kind == DeclarationKind::Interface
                                        && bound.meaning == Meaning::Type
                                        && statement.id == bound.owner)
                                        .then_some(&statement.kind)
                                        .and_then(|statement| match statement {
                                            StatementKind::Interface(interface) => Some(interface),
                                            _ => None,
                                        })
                                })?;
                            if !interface.type_parameters.is_empty()
                                || !interface.extends.is_empty()
                            {
                                return Some(true);
                            }
                            Some(interface.members.iter().any(|member| {
                                if member.recovered || member.recovery_incomplete {
                                    return true;
                                }
                                match file
                                    .bindings
                                    .type_members
                                    .get(&member.id)
                                    .and_then(|member| member.symbol.as_ref())
                                {
                                    Some(TypeMemberSymbol::Named(name)) => {
                                        name.iter().copied().eq(member_name.encode_utf16())
                                    }
                                    Some(_) => false,
                                    None => true,
                                }
                            }))
                        })
                        .unwrap_or(true)
                })
            })
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
    pub semantic_completion: SemanticCompletion,
    pub files: usize,
    pub root_files: usize,
    pub source_files: usize,
    pub root_file_paths: Vec<String>,
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
    pub(crate) syntactic_diagnostics: Vec<Diagnostic>,
    pub(crate) semantic_diagnostics: Vec<Diagnostic>,
    pub emitted_files: Vec<EmittedFile>,
    pub stats: CompileStats,
    pub semantic_completion: SemanticCompletion,
    pub exit_status: CompileExitStatus,
    pub(crate) capabilities: CapabilityAnalysis,
    pub(crate) check_file_completions: Vec<SemanticCompletion>,
    pub(crate) declaration_display_summaries: DeclarationDisplaySummaries,
}
macro_rules! semantic_completions {
    ($default:ident => $default_name:literal, $($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum SemanticCompletion { #[default] $default, $($variant),+ }
        impl SemanticCompletion {
            #[must_use]
            pub const fn combine(self, other: Self) -> Self { if self as u8 >= other as u8 { self } else { other } }
            #[must_use]
            pub const fn is_complete(self) -> bool { matches!(self, Self::$default) }
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { Self::$default => $default_name, $(Self::$variant => $name),+ }
            }
        }
    };
}
semantic_completions! { Complete => "complete", Deferred => "deferred", Cycle => "cycle", Limit => "limit" }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileExitStatus {
    Success = 0,
    DiagnosticsPresentOutputsSkipped = 1,
    DiagnosticsPresentOutputsGenerated = 2,
    SemanticIncomplete = 3,
}
impl CompileExitStatus {
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
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
            .map(|input| display_path(&input.host_path))
            .collect::<Vec<_>>();
        self.compile_inputs(
            inputs,
            options,
            Vec::new(),
            Vec::new(),
            ProjectResolution::Complete,
            ProjectProvenance::default(),
            ProjectStats {
                root_files: root_file_paths.len(),
                root_file_paths,
                project_configs: 0,
                project_references: 0,
            },
        )
    }
    pub fn compile_resolved(
        &self,
        resolved: ResolvedProject,
        options: &CompilerOptions,
    ) -> CompileOutput {
        let target_overridden = options.target != resolved.options.target;
        let stats = ProjectStats {
            root_files: resolved.root_files.len(),
            root_file_paths: resolved
                .root_files
                .iter()
                .map(|path| display_path(path))
                .collect(),
            project_configs: resolved.project_config_count,
            project_references: resolved.project_reference_count,
        };
        let mut provenance = resolved.provenance;
        if target_overridden {
            provenance.clear_option_origin(CompilerOptionKey::Target);
        }
        let config_diagnostics = resolved
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.phase != DiagnosticPhase::Program)
            .cloned()
            .collect();
        let program_diagnostics = resolved
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.phase != DiagnosticPhase::Config)
            .collect();
        self.compile_inputs(
            resolved.inputs,
            options,
            config_diagnostics,
            program_diagnostics,
            resolved.resolution,
            provenance,
            stats,
        )
    }
    fn compile_inputs(
        &self,
        mut inputs: Vec<SourceInput>,
        options: &CompilerOptions,
        config_diagnostics: Vec<Diagnostic>,
        mut program_diagnostics: Vec<Diagnostic>,
        resolution: ProjectResolution,
        provenance: ProjectProvenance,
        project_stats: ProjectStats,
    ) -> CompileOutput {
        let total_start = Instant::now();
        let mut source_order_keys = Vec::with_capacity(inputs.len());
        let mut seen_source_keys = BTreeSet::new();
        for input in &inputs {
            let key = display_path(&input.host_path);
            if seen_source_keys.insert(key.clone()) {
                source_order_keys.push(key);
            }
        }
        inputs.sort_by_cached_key(|input| {
            (display_path(&input.host_path), display_path(&input.path))
        });
        inputs.dedup_by(|left, right| {
            display_path(&left.host_path) == display_path(&right.host_path)
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
            .map(|source| (display_path(&source.host_path), source.id))
            .collect::<BTreeMap<_, _>>();
        let source_order = source_order_keys
            .iter()
            .filter_map(|key| source_ids.get(key).copied())
            .collect::<Vec<_>>();
        let jobs: Vec<_> = sources
            .into_par_iter()
            .map(|source| {
                let parse_start = Instant::now();
                let parsed = parse_source(&source);
                let parse_time = parse_start.elapsed();
                let bind_start = Instant::now();
                let bindings = bind_source_with_kind(source.id, source.kind(), &parsed.unit);
                let bind_time = bind_start.elapsed();
                (
                    ProgramFile {
                        source,
                        syntax: parsed.unit,
                        bindings,
                    },
                    parsed.diagnostics,
                    parse_time,
                    bind_time,
                )
            })
            .collect();
        let (option_diagnostics, fatal_option_diagnostics) =
            compiler_option_diagnostics(options, &provenance);
        let has_fatal_option_error = !fatal_option_diagnostics.is_empty();
        let mut files = Vec::with_capacity(jobs.len());
        let mut syntax_diagnostics = Vec::new();
        let mut parse_time = Duration::ZERO;
        let mut bind_time = Duration::ZERO;
        for (file, diagnostics, parsed, bound) in jobs {
            syntax_diagnostics.extend(diagnostics);
            parse_time += parsed;
            bind_time += bound;
            files.push(file);
        }
        sort_and_deduplicate(&mut syntax_diagnostics);
        program_diagnostics.extend(option_diagnostics);
        sort_and_deduplicate(&mut program_diagnostics);
        files.sort_by_key(|file| file.source.id);
        let program = build_program(files, source_order, options);
        let missing_essential_types = if program.files.is_empty() {
            Vec::new()
        } else {
            program.missing_essential_global_types()
        };
        let has_missing_essential_types = !missing_essential_types.is_empty();
        let capabilities = CapabilityAnalysis::derive_with_javascript_assignments(
            &program.files,
            options,
            CapabilityContext {
                has_fatal_option_error,
                has_missing_essential_types,
            },
            &program.javascript_assignments,
        );
        let has_checkable_file = !program.standard_library_type_alias_collisions.is_empty()
            || program.files.iter().any(|file| {
                !file.syntax.contextual_grammar_facts().is_empty()
                    || capabilities.semantic_check_file_is_enabled(file.source.id)
                        && (file.syntax.statements.iter().any(|statement| {
                            let mut checkable = false;
                            statement.for_each_statement(&mut |statement| {
                                checkable |= capabilities
                                    .semantic_check_node_is_claimed(file.source.id, statement.id);
                            });
                            checkable
                        }) || capabilities.has_claimed_function_like(file.source.id))
            });
        let terminal = has_fatal_option_error || resolution == ProjectResolution::Terminal;
        let emit_plan = if terminal {
            EmitPlan::empty(program.files.len())
        } else {
            EmitPlan::for_program(&program.files, options, &provenance, &capabilities)
        };
        program_diagnostics.extend(emit_plan.program_diagnostics().iter().cloned());
        program_diagnostics.extend(
            missing_essential_types
                .into_iter()
                .map(|name| Diagnostic::global(format!("Cannot find global type '{name}'."), 2318)),
        );
        program_diagnostics.extend(emit_plan.emit_diagnostics().iter().cloned());
        sort_and_deduplicate(&mut program_diagnostics);
        let unchecked = |completion, declaration_display_summaries| CheckResult {
            diagnostics: Vec::new(),
            type_count: 0,
            semantic_completion: completion,
            file_semantic_completions: vec![completion; program.files.len()],
            declaration_display_summaries,
        };
        let check_start = Instant::now();
        let CheckResult {
            diagnostics: mut semantic_diagnostics,
            type_count,
            semantic_completion: mut checker_completion,
            file_semantic_completions,
            declaration_display_summaries,
        } = if options.no_check || !has_checkable_file {
            if terminal {
                unchecked(
                    SemanticCompletion::Deferred,
                    DeclarationDisplaySummaries::default(),
                )
            } else {
                unchecked(
                    SemanticCompletion::Complete,
                    summarize_program(&program, options, &capabilities),
                )
            }
        } else if terminal {
            unchecked(
                SemanticCompletion::Deferred,
                DeclarationDisplaySummaries::default(),
            )
        } else {
            check_program(&program, options, &capabilities)
        };
        let check_time = check_start.elapsed();
        sort_and_deduplicate_by_path(&mut semantic_diagnostics);
        if !terminal && !capabilities.semantic_diagnostics_are_claimed(options) {
            checker_completion = checker_completion.combine(SemanticCompletion::Deferred);
        }
        let emit_start = Instant::now();
        let syntactic_completion = if capabilities.syntactic_diagnostics_are_claimed() {
            SemanticCompletion::Complete
        } else {
            SemanticCompletion::Deferred
        };
        let mut diagnostics = if has_fatal_option_error {
            fatal_option_diagnostics
        } else {
            config_diagnostics
        };
        if !terminal {
            diagnostic_products::DiagnosticPhaseProducts([
                &syntax_diagnostics,
                &program_diagnostics,
                &semantic_diagnostics,
            ])
            .append_to(&mut diagnostics);
        }
        sort_and_deduplicate_for_cli(&mut diagnostics);
        let mut semantic_completion = if terminal {
            SemanticCompletion::Complete
        } else {
            syntactic_completion.combine(checker_completion)
        };
        if !terminal && !capabilities.requested_emit_is_claimed(&program.files, options) {
            semantic_completion = semantic_completion.combine(SemanticCompletion::Deferred);
        }
        let has_errors = diagnostics
            .iter()
            .any(|diagnostic| diagnostic.category == DiagnosticCategory::Error);
        let emit_suppressed = options.no_emit || terminal || has_errors && options.no_emit_on_error;
        let mut emitted_files = if emit_suppressed {
            Vec::new()
        } else {
            program
                .files
                .par_iter()
                .flat_map_iter(|file| {
                    emit_file_with_plan(
                        file,
                        options,
                        emit_plan.for_file(file.source.id),
                        &declaration_display_summaries,
                    )
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
        let (lines, symbols) = program.files.iter().fold((0, 0), |(lines, symbols), file| {
            (
                lines + file.source.text.lines().count(),
                symbols + file.bindings.declarations.len(),
            )
        });
        let stats = CompileStats {
            semantic_completion,
            files: program.files.len(),
            root_files: project_stats.root_files,
            source_files: program.files.len(),
            root_file_paths: project_stats.root_file_paths,
            source_file_paths: program
                .source_order
                .iter()
                .map(|id| display_path(&program.files[id.0 as usize].source.host_path))
                .collect(),
            project_configs: project_stats.project_configs,
            project_references: project_stats.project_references,
            lines,
            identifiers: symbols,
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
        } else if resolution == ProjectResolution::Terminal {
            CompileExitStatus::DiagnosticsPresentOutputsSkipped
        } else if program.files.is_empty()
            && (project_stats.root_files > 0 || project_stats.project_configs > 0)
        {
            CompileExitStatus::DiagnosticsPresentOutputsGenerated
        } else if options.no_emit
            || options.no_emit_on_error
            || terminal
            || !emit_plan.emit_diagnostics().is_empty()
        {
            CompileExitStatus::DiagnosticsPresentOutputsSkipped
        } else {
            CompileExitStatus::DiagnosticsPresentOutputsGenerated
        };
        CompileOutput {
            program,
            diagnostics,
            syntactic_diagnostics: syntax_diagnostics,
            semantic_diagnostics,
            emitted_files,
            stats,
            semantic_completion,
            exit_status,
            capabilities,
            check_file_completions: file_semantic_completions,
            declaration_display_summaries,
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
) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut fatal_diagnostics = Vec::new();
    let target_origin = provenance.option_origin(CompilerOptionKey::Target);
    match classify_target_value(&options.target) {
        TargetValueOutcome::Accepted => {}
        TargetValueOutcome::Invalid { message, code } => {
            let message = message.to_string();
            if target_origin.is_none() {
                fatal_diagnostics.push(Diagnostic::global(message, code));
            }
        }
        TargetValueOutcome::Removed { message, code } => fatal_diagnostics.push(
            match provenance.program_option_origin(CompilerOptionKey::Target, None) {
                Some(origin) => origin.diagnostic_at_value(message.to_string(), code),
                None => Diagnostic::global(message.to_string(), code),
            },
        ),
    }
    let option_dependency = |primary, secondary, message| match provenance
        .program_option_origin(primary, Some(secondary))
    {
        Some(origin) => origin.diagnostic_at_key(message, 5052),
        None => Diagnostic::global(message, 5052),
    };
    if options.strict_property_initialization == Some(true)
        && !options.effective_strict_null_checks()
    {
        let message = concat!(
            "Option 'strictPropertyInitialization' cannot be specified without specifying ",
            "option 'strictNullChecks'."
        )
        .to_string();
        diagnostics.push(option_dependency(
            CompilerOptionKey::StrictPropertyInitialization,
            CompilerOptionKey::StrictNullChecks,
            message,
        ));
    }
    if options.check_js == Some(true) && !options.allow_js {
        let message =
            "Option 'checkJs' cannot be specified without specifying option 'allowJs'.".to_string();
        diagnostics.push(option_dependency(
            CompilerOptionKey::CheckJs,
            CompilerOptionKey::AllowJs,
            message,
        ));
    }
    (diagnostics, fatal_diagnostics)
}
fn build_program(
    mut files: Vec<ProgramFile>,
    source_order: Vec<FileId>,
    options: &CompilerOptions,
) -> Program {
    let import_aliases = import_aliases::ImportAliases::build(&files, options.allow_js);
    let (global_values, global_types) =
        global_declarations(source_order.iter().map(|id| &files[id.0 as usize]));
    let mut standard_library = StandardLibraryEnvironment::from_options(options);
    for file in &mut files {
        file.bindings.finalize_flow(&file.syntax, |name| {
            global_values
                .get(name)
                .and_then(|declarations| declarations.first().copied())
                .or_else(|| standard_library.resolve(name, Meaning::Value))
        });
    }
    let javascript_assignments = JavaScriptAssignments::build(&files, &global_values);
    let standard_library_type_alias_collisions = global_types
        .iter()
        .filter_map(|(name, declarations)| {
            let [authored] = declarations.as_slice() else {
                return None;
            };
            let bound = files[authored.file.0 as usize]
                .bindings
                .declaration(*authored)?;
            if !matches!(
                bound.kind,
                DeclarationKind::Interface | DeclarationKind::TypeAlias
            ) {
                return None;
            }
            let library = standard_library.resolve(name, Meaning::Type)?;
            standard_library.homogeneous_record_origin(library)?;
            standard_library.hide_homogeneous_record_type(library);
            Some((*authored, library))
        })
        .collect();
    Program {
        source_order,
        files,
        global_values,
        global_types,
        standard_library,
        standard_library_type_alias_collisions,
        javascript_assignments,
        import_aliases,
    }
}
type GlobalDeclarations = BTreeMap<String, Vec<DeclId>>;

fn global_declarations<'a>(
    source_files: impl IntoIterator<Item = &'a ProgramFile>,
) -> (GlobalDeclarations, GlobalDeclarations) {
    let mut global_values = GlobalDeclarations::new();
    let mut global_types = GlobalDeclarations::new();
    for file in source_files {
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
    (global_values, global_types)
}
fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
