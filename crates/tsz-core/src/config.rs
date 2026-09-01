//! TypeScript-compatible project selection and JSONC configuration loading.
mod compiler_options;

use crate::diagnostics::{Diagnostic, DiagnosticPhase, RelatedInformation, sort_and_deduplicate};
use crate::host::ProgramHost;
use crate::program::{CompilerOptions, DeferredCompilerOption, SourceInput};
use crate::source::{
    FileId, SourceText, display_path, normalize_project_path_lexically as normalize_path,
};
use crate::syntax::{Token, TokenKind, scan_source};
pub use compiler_options::{
    CompilerOptionKey, CompilerOptionPatch, TargetValueOutcome, classify_target_value,
};
use compiler_options::{CompilerOptionOrigin, decode_compiler_options};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
const CONFIG_FILE_NAME: &str = "tsconfig.json";
const DEFAULT_INCLUDE: &str = "**/*";
/// How a process or service selects the entry program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSelection {
    /// Command-line roots. Configuration discovery is intentionally bypassed.
    Files(Vec<PathBuf>),
    /// An explicit configuration file or a directory containing `tsconfig.json`.
    Project(PathBuf),
    /// Search this directory and its ancestors for `tsconfig.json`.
    Search(PathBuf),
}
/// Host-backed project resolution request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRequest {
    pub selection: ProjectSelection,
    /// Discovery override; the adapter applies other overrides after resolution.
    pub overrides: CompilerOptionPatch,
}
impl ProjectRequest {
    #[must_use]
    pub fn new(selection: ProjectSelection) -> Self {
        Self {
            selection,
            overrides: CompilerOptionPatch::default(),
        }
    }
}
/// Fully resolved roots and configuration metadata, before parsing sources.
#[derive(Debug)]
pub struct ResolvedProject {
    pub options: CompilerOptions,
    pub inputs: Vec<SourceInput>,
    pub root_files: Vec<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    pub entry_config: Option<PathBuf>,
    pub project_config_count: usize,
    pub project_reference_count: usize,
    pub(crate) resolution: ProjectResolution,
    pub(crate) provenance: ProjectProvenance,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ProjectResolution {
    #[default]
    Complete,
    Terminal,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct ProjectProvenance {
    current_directory: PathBuf,
    entry_config_path: Option<PathBuf>,
    option_origins: BTreeMap<CompilerOptionKey, CompilerOptionOrigin>,
    entry_option_origins: BTreeMap<CompilerOptionKey, CompilerOptionOrigin>,
    entry_compiler_options_origin: Option<CompilerOptionOrigin>,
    root_reasons: BTreeMap<String, RootReason>,
    case_sensitive: bool,
}
impl ProjectProvenance {
    pub(crate) fn entry_config_path(&self) -> Option<&Path> {
        self.entry_config_path.as_deref()
    }
    pub(crate) fn option_origin(&self, key: CompilerOptionKey) -> Option<&CompilerOptionOrigin> {
        self.option_origins.get(&key)
    }
    pub(crate) fn clear_option_origin(&mut self, key: CompilerOptionKey) {
        self.option_origins.remove(&key);
    }
    pub(crate) fn entry_option_origin(
        &self,
        key: CompilerOptionKey,
    ) -> Option<&CompilerOptionOrigin> {
        let config_path = self.entry_config_path.as_deref()?;
        self.option_origin(key)
            .filter(|origin| origin.belongs_to(config_path, &self.current_directory))
    }
    pub(crate) fn program_option_origin(
        &self,
        primary: CompilerOptionKey,
        secondary: Option<CompilerOptionKey>,
    ) -> Option<&CompilerOptionOrigin> {
        self.entry_option_origins
            .get(&primary)
            .or_else(|| secondary.and_then(|key| self.entry_option_origins.get(&key)))
            .or(self.entry_compiler_options_origin.as_ref())
    }
    pub(crate) fn root_reason(&self, path: &Path) -> Option<RootReason> {
        self.root_reasons
            .get(&path_key(path, self.case_sensitive))
            .copied()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootReason {
    CommandLine,
    FilesList,
}
impl RootReason {
    pub(crate) const fn diagnostic(self) -> (&'static str, u32) {
        match self {
            Self::CommandLine => ("Root file specified for compilation", 1427),
            Self::FilesList => ("Part of 'files' list in tsconfig.json", 1409),
        }
    }
}
#[derive(Debug, Clone)]
struct RootMetadata {
    display_path: String,
    logical_path: PathBuf,
    reason: RootReason,
}
impl ResolvedProject {
    /// Apply explicit overrides and clear provenance only for supplied keys.
    #[must_use]
    pub fn apply_option_patch(&mut self, patch: &CompilerOptionPatch) -> CompilerOptions {
        for key in CompilerOptionKey::ALL
            .iter()
            .copied()
            .filter(|key| patch.contains(*key))
        {
            self.provenance.option_origins.remove(&key);
        }
        let mut options = self.options.clone();
        patch.apply_to(&mut options);
        options
    }
}
/// Resolve inherited options/references/roots; `exclude` never filters literal `files`.
#[must_use]
pub fn resolve_project(host: &dyn ProgramHost, request: &ProjectRequest) -> ResolvedProject {
    let mut resolver = Resolver::new(host, request);
    let (options, roots) = match &request.selection {
        ProjectSelection::Files(files) => {
            let roots = files
                .iter()
                .map(|path| {
                    let absolute = absolute_path(host.current_directory(), path);
                    resolver.record_root(
                        &absolute,
                        display_path(&normalize_path(path)),
                        normalize_path(path),
                        RootReason::CommandLine,
                    );
                    absolute
                })
                .collect();
            let mut options = CompilerOptions::default();
            resolver.apply_overrides(&CompilerOptionPatch::default(), &mut options);
            (options, roots)
        }
        ProjectSelection::Project(path) => resolver.resolve_explicit_project(path),
        ProjectSelection::Search(start) => find_config_file(host, start).map_or_else(
            || (CompilerOptions::default(), Vec::new()),
            |candidate| resolver.resolve_config_entry(candidate),
        ),
    };
    let root_files = deduplicate_paths(roots, host.use_case_sensitive_file_names());
    let mut config_diagnostics = std::mem::take(&mut resolver.diagnostics);
    let mut program_diagnostics = std::mem::take(&mut resolver.program_diagnostics);
    let mut inputs = Vec::with_capacity(root_files.len());
    for path in &root_files {
        let metadata = resolver.root_metadata(path);
        if !host.file_exists(path) {
            program_diagnostics.push(root_file_diagnostic(
                format!("File '{}' not found.", metadata.display_path),
                6053,
                metadata.reason,
            ));
            continue;
        }
        if !supported_source_file(path, options.allow_js) {
            let (code, message) =
                unsupported_root_message(&metadata.display_path, path, options.allow_js);
            if metadata.reason == RootReason::CommandLine {
                let absolute_display = display_path(path);
                if absolute_display != metadata.display_path {
                    let (_, absolute_message) =
                        unsupported_root_message(&absolute_display, path, options.allow_js);
                    program_diagnostics.push(root_file_diagnostic(
                        absolute_message,
                        code,
                        metadata.reason,
                    ));
                }
            }
            program_diagnostics.push(root_file_diagnostic(message, code, metadata.reason));
            continue;
        }
        match host.read_file(path) {
            Ok(text) => {
                inputs.push(SourceInput::with_host_path(
                    metadata.logical_path,
                    path.clone(),
                    Arc::<str>::from(text),
                ));
            }
            Err(_) => program_diagnostics.push(root_file_diagnostic(
                format!("File '{}' not found.", metadata.display_path),
                6053,
                metadata.reason,
            )),
        }
    }
    sort_and_deduplicate(&mut config_diagnostics);
    sort_and_deduplicate(&mut program_diagnostics);
    for diagnostic in &mut program_diagnostics {
        diagnostic.phase = DiagnosticPhase::Program;
    }
    let mut diagnostics = config_diagnostics;
    diagnostics.extend(program_diagnostics);
    sort_and_deduplicate(&mut diagnostics);
    let entry_config_path = resolver.entry_config.clone();
    let root_reasons = resolver
        .roots
        .iter()
        .map(|(key, metadata)| (key.clone(), metadata.reason))
        .collect();
    let provenance = ProjectProvenance {
        current_directory: host.current_directory().to_path_buf(),
        entry_config_path,
        option_origins: resolver.option_origins,
        entry_option_origins: resolver.entry_option_origins,
        entry_compiler_options_origin: resolver.entry_compiler_options_origin,
        root_reasons,
        case_sensitive: host.use_case_sensitive_file_names(),
    };
    ResolvedProject {
        options,
        inputs,
        root_files,
        diagnostics,
        entry_config: resolver.entry_config,
        project_config_count: resolver.project_config_count,
        project_reference_count: resolver.project_reference_count,
        resolution: resolver.resolution,
        provenance,
    }
}
/// Search ancestors for `tsconfig.json` without parsing or expanding the project.
#[must_use]
pub fn find_config_file(host: &dyn ProgramHost, start: &Path) -> Option<PathBuf> {
    let absolute = absolute_path(host.current_directory(), start);
    let directory = if host.directory_exists(&absolute) {
        absolute
    } else {
        absolute.parent()?.to_path_buf()
    };
    directory
        .ancestors()
        .map(|directory| directory.join(CONFIG_FILE_NAME))
        .find(|candidate| host.file_exists(candidate))
        .map(|candidate| normalize_path(&candidate))
}
struct Resolver<'a> {
    host: &'a dyn ProgramHost,
    overrides: CompilerOptionPatch,
    diagnostics: Vec<Diagnostic>,
    program_diagnostics: Vec<Diagnostic>,
    resolution: ProjectResolution,
    entry_config: Option<PathBuf>,
    project_config_count: usize,
    project_reference_count: usize,
    seen_configs: BTreeSet<String>,
    cache: BTreeMap<String, LoadedConfig>,
    incomplete_configs: BTreeSet<String>,
    roots: BTreeMap<String, RootMetadata>,
    option_origins: BTreeMap<CompilerOptionKey, CompilerOptionOrigin>,
    entry_option_origins: BTreeMap<CompilerOptionKey, CompilerOptionOrigin>,
    entry_compiler_options_origin: Option<CompilerOptionOrigin>,
}
impl<'a> Resolver<'a> {
    fn new(host: &'a dyn ProgramHost, request: &ProjectRequest) -> Self {
        let absolute = |path: &Path| absolute_path(host.current_directory(), path);
        let mut overrides = request.overrides.clone();
        overrides.out_dir = overrides.out_dir.as_deref().map(absolute);
        overrides.declaration_dir = overrides.declaration_dir.as_deref().map(absolute);
        overrides.absolutize_deferred_paths(host.current_directory());
        Self {
            host,
            overrides,
            diagnostics: Vec::new(),
            program_diagnostics: Vec::new(),
            resolution: ProjectResolution::Complete,
            entry_config: None,
            project_config_count: 0,
            project_reference_count: 0,
            seen_configs: BTreeSet::new(),
            cache: BTreeMap::new(),
            incomplete_configs: BTreeSet::new(),
            roots: BTreeMap::new(),
            option_origins: BTreeMap::new(),
            entry_option_origins: BTreeMap::new(),
            entry_compiler_options_origin: None,
        }
    }
    fn apply_overrides(&mut self, configured: &CompilerOptionPatch, options: &mut CompilerOptions) {
        let explicit_allow_js = configured.allow_js.is_some() || self.overrides.allow_js.is_some();
        for key in CompilerOptionKey::ALL
            .iter()
            .filter(|key| self.overrides.contains(**key))
        {
            self.option_origins.remove(key);
        }
        self.overrides.apply_to(options);
        if !explicit_allow_js && options.check_js == Some(true) {
            options.allow_js = true;
        }
    }
    fn record_root(
        &mut self,
        host_path: &Path,
        display_path: String,
        logical_path: PathBuf,
        reason: RootReason,
    ) {
        self.roots
            .entry(path_key(
                host_path,
                self.host.use_case_sensitive_file_names(),
            ))
            .or_insert(RootMetadata {
                display_path,
                logical_path,
                reason,
            });
    }
    fn root_metadata(&self, host_path: &Path) -> RootMetadata {
        self.roots
            .get(&path_key(
                host_path,
                self.host.use_case_sensitive_file_names(),
            ))
            .cloned()
            .unwrap_or_else(|| RootMetadata {
                display_path: display_path(host_path),
                logical_path: logical_source_path_from_host(self.host, host_path),
                reason: RootReason::CommandLine,
            })
    }
    fn resolve_explicit_project(&mut self, requested: &Path) -> (CompilerOptions, Vec<PathBuf>) {
        let absolute = absolute_path(self.host.current_directory(), requested);
        let config_path =
            if requested.as_os_str().is_empty() || self.host.directory_exists(&absolute) {
                let candidate = absolute.join(CONFIG_FILE_NAME);
                if !self.host.file_exists(&candidate) {
                    self.resolution = ProjectResolution::Terminal;
                    self.diagnostics.push(Diagnostic::global(
                        format!(
                            "Cannot find a tsconfig.json file at the current directory: {}.",
                            display_path(&candidate)
                        ),
                        5081,
                    ));
                    return (CompilerOptions::default(), Vec::new());
                }
                candidate
            } else {
                if !self.host.file_exists(&absolute) {
                    self.resolution = ProjectResolution::Terminal;
                    self.diagnostics.push(Diagnostic::global(
                        format!(
                            "The specified path does not exist: '{}'.",
                            display_path(requested)
                        ),
                        5058,
                    ));
                    return (CompilerOptions::default(), Vec::new());
                }
                absolute
            };
        self.resolve_config_entry(config_path)
    }
    fn resolve_config_entry(&mut self, config_path: PathBuf) -> (CompilerOptions, Vec<PathBuf>) {
        let config_path = normalize_path(&config_path);
        if !self.host.file_exists(&config_path) {
            self.diagnostics.push(Diagnostic::global(
                format!("Cannot read file '{}'.", display_path(&config_path)),
                5083,
            ));
            return (CompilerOptions::default(), Vec::new());
        }
        self.entry_config = Some(config_path.clone());
        let mut stack = Vec::new();
        let Some(loaded) = self.load_config(&config_path, &mut stack, true) else {
            return (CompilerOptions::default(), Vec::new());
        };
        self.diagnostics.extend(
            loaded
                .merged
                .deferred_option_diagnostics
                .values()
                .filter_map(Clone::clone),
        );
        let mut options = CompilerOptions::default();
        loaded.merged.options.apply_to(&mut options);
        self.option_origins = loaded.merged.option_origins.clone();
        self.apply_overrides(&loaded.merged.options, &mut options);
        let roots = self.resolve_root_files(&loaded, &options);
        (options, roots)
    }
    fn load_config(
        &mut self,
        path: &Path,
        stack: &mut Vec<PathBuf>,
        is_entry: bool,
    ) -> Option<LoadedConfig> {
        let path = normalize_path(path);
        let key = path_key(&path, self.host.use_case_sensitive_file_names());
        if let Some(cycle_start) = stack
            .iter()
            .position(|item| path_key(item, self.host.use_case_sensitive_file_names()) == key)
        {
            self.incomplete_configs.extend(
                stack[cycle_start..]
                    .iter()
                    .map(|item| path_key(item, self.host.use_case_sensitive_file_names())),
            );
            self.incomplete_configs.insert(key);
            let mut chain: Vec<String> = stack[cycle_start..]
                .iter()
                .map(|item| display_path(item))
                .collect();
            chain.push(display_path(&path));
            self.diagnostics.push(Diagnostic::global(
                format!(
                    "Circularity detected while resolving configuration: {}",
                    chain.join(" -> ")
                ),
                18000,
            ));
            return None;
        }
        if let Some(cached) = self.cache.get(&key) {
            return Some(cached.clone());
        }
        if self.seen_configs.insert(key.clone()) {
            self.project_config_count += 1;
        }
        let loaded = self.host.read_file(&path).ok().and_then(|text| {
            let text = Arc::<str>::from(text);
            parse_jsonc(&text).ok().map(|document| (text, document))
        });
        let Some((text, document)) = loaded else {
            self.diagnostics.push(Diagnostic::global(
                format!("Cannot read file '{}'.", display_path(&path)),
                5083,
            ));
            return None;
        };
        let object = document.value.as_object().cloned().unwrap_or_default();
        stack.push(path.clone());
        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        let extends_values = string_values(object.get("extends"), true).unwrap_or_default();
        let mut merged = MergedConfig::default();
        let mut bases_complete = true;
        for raw in &extends_values {
            let Some(extends_path) = self.resolve_extends_path(directory, raw) else {
                continue;
            };
            if let Some(base) = self.load_config(&extends_path, stack, false) {
                merged.merge_from(&base.merged);
                bases_complete &= base.complete;
            }
        }
        let logical_path = logical_path_from_host(self.host.current_directory(), &path);
        let decoded = decode_compiler_options(
            &document.source_spans.compiler_options,
            directory,
            &logical_path,
            &text,
            &mut self.diagnostics,
        );
        if is_entry {
            self.entry_option_origins = decoded.authored_option_origins;
            self.entry_compiler_options_origin =
                document.source_spans.compiler_options_key.map(|span| {
                    CompilerOptionOrigin::for_compiler_options_key(
                        display_path(&logical_path),
                        Arc::clone(&text),
                        span,
                    )
                });
        }
        merged.options.merge_from(&decoded.patch);
        merged.option_origins.extend(decoded.option_origins);
        merged
            .deferred_option_diagnostics
            .extend(decoded.deferred_diagnostics);
        if let Some(values) = string_values(object.get("files"), false) {
            merged.files = Some(Selector::new(values, directory));
        }
        if let Some(values) = string_values(object.get("include"), false) {
            merged.include = Some(Selector::new(values, directory));
        }
        if let Some(values) = string_values(object.get("exclude"), false) {
            merged.exclude = Some(Selector::new(values, directory));
        }
        merged.own_has_extends = object.contains_key("extends");
        merged.own_has_references = object.contains_key("references");
        let references = if is_entry {
            project_references(&object, &document.source_spans.references, directory)
        } else {
            Vec::new()
        };
        if is_entry {
            self.project_reference_count = references.len();
        }
        for reference in &references {
            let resolved_config_path = if reference
                .path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                reference.path.clone()
            } else {
                reference.path.join(CONFIG_FILE_NAME)
            };
            let exists = self.host.file_exists(&resolved_config_path);
            if !exists {
                self.program_diagnostics.push(Diagnostic::error_at_text(
                    display_path(&logical_path_from_host(
                        self.host.current_directory(),
                        &path,
                    )),
                    reference.source_start,
                    reference.source_length,
                    Arc::clone(&text),
                    format!("File '{}' not found.", display_path(&reference.path)),
                    6053,
                ));
            }
        }
        stack.pop();
        let loaded = LoadedConfig {
            merged,
            complete: bases_complete && !self.incomplete_configs.contains(&key),
        };
        if loaded.complete {
            self.cache.insert(key, loaded.clone());
        }
        Some(loaded)
    }
    fn resolve_extends_path(&mut self, directory: &Path, raw: &str) -> Option<PathBuf> {
        let relative = raw.starts_with("./") || raw.starts_with("../");
        let raw_path = Path::new(raw);
        if relative || raw_path.is_absolute() {
            let candidate = absolute_path(directory, raw_path);
            if self.host.file_exists(&candidate) {
                return Some(candidate);
            }
            if candidate.extension().is_none() {
                let json_candidate = PathBuf::from(format!("{}.json", candidate.display()));
                if self.host.file_exists(&json_candidate) {
                    return Some(json_candidate);
                }
            }
            self.diagnostics
                .push(Diagnostic::global(format!("File '{raw}' not found."), 6053));
            return None;
        }
        let mut ancestor = Some(directory);
        while let Some(base) = ancestor {
            let package = base.join("node_modules").join(raw);
            for candidate in package_config_candidates(self.host, &package) {
                if self.host.file_exists(&candidate) {
                    return Some(normalize_path(&candidate));
                }
            }
            ancestor = base.parent();
        }
        self.diagnostics
            .push(Diagnostic::global(format!("File '{raw}' not found."), 6053));
        None
    }
    fn resolve_root_files(
        &mut self,
        loaded: &LoadedConfig,
        options: &CompilerOptions,
    ) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(selector) = &loaded.merged.files {
            for value in &selector.values {
                let path = absolute_path(&selector.origin, Path::new(value));
                self.record_root(
                    &path,
                    display_path(&path),
                    logical_source_path_from_host(self.host, &path),
                    RootReason::FilesList,
                );
                roots.push(path);
            }
        }
        let include = if loaded.merged.files.is_none() && loaded.merged.include.is_none() {
            Some(Selector::new(
                vec![DEFAULT_INCLUDE.to_string()],
                self.entry_config
                    .as_deref()
                    .and_then(Path::parent)
                    .unwrap_or_else(|| Path::new(".")),
            ))
        } else {
            loaded.merged.include.clone()
        };
        let exclude = loaded.merged.exclude.clone().unwrap_or_else(|| {
            let values = [options.out_dir.as_ref(), options.declaration_dir.as_ref()]
                .into_iter()
                .flatten()
                .map(|path| display_path(path))
                .collect();
            Selector::new(values, Path::new("."))
        });
        if let Some(include) = &include {
            let wildcard_roots =
                discover_wildcard_files(self.host, include, &exclude, options.allow_js, &roots);
            roots.extend(wildcard_roots);
        }
        let reference_count = self.project_reference_count;
        if loaded
            .merged
            .files
            .as_ref()
            .is_some_and(|selector| selector.values.is_empty())
            && reference_count == 0
            && !loaded.merged.own_has_extends
        {
            let config = self.entry_config.as_ref().expect("entry config is set");
            self.diagnostics.push(Diagnostic::global(
                format!(
                    "The 'files' list in config file '{}' is empty.",
                    display_path(config)
                ),
                18002,
            ));
        } else if roots.is_empty()
            && loaded.merged.files.is_none()
            && !loaded.merged.own_has_references
        {
            let config = self.entry_config.as_ref().expect("entry config is set");
            let include_values = include
                .as_ref()
                .map_or_else(Vec::new, |selector| selector.values.clone());
            self.diagnostics.push(Diagnostic::global(
                format!(
                    "No inputs were found in config file '{}'. Specified 'include' paths were '{}' and 'exclude' paths were '{}'.",
                    display_path(config),
                    json_array(&include_values),
                    json_array(&exclude.values)
                ),
                18003,
            ));
        }
        roots
    }
}
#[derive(Debug, Clone)]
struct LoadedConfig {
    merged: MergedConfig,
    /// Cache only complete loads; cycle recovery remains traversal-local.
    complete: bool,
}
#[derive(Debug, Clone, Default)]
struct MergedConfig {
    options: CompilerOptionPatch,
    option_origins: BTreeMap<CompilerOptionKey, CompilerOptionOrigin>,
    deferred_option_diagnostics: BTreeMap<DeferredCompilerOption, Option<Diagnostic>>,
    files: Option<Selector>,
    include: Option<Selector>,
    exclude: Option<Selector>,
    own_has_extends: bool,
    own_has_references: bool,
}
impl MergedConfig {
    fn merge_from(&mut self, other: &Self) {
        self.options.merge_from(&other.options);
        self.option_origins.extend(other.option_origins.clone());
        self.deferred_option_diagnostics
            .extend(other.deferred_option_diagnostics.clone());
        if other.files.is_some() {
            self.files.clone_from(&other.files);
        }
        if other.include.is_some() {
            self.include.clone_from(&other.include);
        }
        if other.exclude.is_some() {
            self.exclude.clone_from(&other.exclude);
        }
    }
}
#[derive(Debug, Clone)]
struct Selector {
    values: Vec<String>,
    origin: PathBuf,
}
impl Selector {
    fn new(values: Vec<String>, origin: &Path) -> Self {
        Self {
            values,
            origin: normalize_path(origin),
        }
    }
}
fn project_references(
    object: &Map<String, Value>,
    spans: &[(u32, u32)],
    origin: &Path,
) -> Vec<ProjectReference> {
    object
        .get("references")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .enumerate()
        .filter_map(|(index, reference)| {
            let raw = reference.get("path").and_then(Value::as_str)?;
            let (source_start, source_length) = spans.get(index).copied().unwrap_or((0, 0));
            Some(ProjectReference {
                path: absolute_path(origin, Path::new(raw)),
                source_start,
                source_length,
            })
        })
        .collect()
}
struct ProjectReference {
    path: PathBuf,
    source_start: u32,
    source_length: u32,
}
struct JsoncDocument {
    value: Value,
    source_spans: ConfigSourceSpans,
}
#[derive(Debug, Default)]
struct ConfigSourceSpans {
    compiler_options: Vec<ConfigOptionOccurrence>,
    compiler_options_key: Option<(u32, u32)>,
    references: Vec<(u32, u32)>,
}
#[derive(Debug, Clone, PartialEq)]
struct ConfigOptionOccurrence {
    name: String,
    value: Value,
    span: ConfigOptionSpans,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConfigOptionSpans {
    key: (u32, u32),
    value: Option<(u32, u32)>,
}
impl ConfigSourceSpans {
    /// Inventory source provenance while walking the shared JSONC token stream once.
    fn from_tokens(source_text: &str, normalized: &str, tokens: &[Token]) -> Self {
        let mut spans = Self::default();
        let mut object_depth = 0usize;
        let mut array_depth = 0usize;
        let mut options_seen = false;
        let mut references_seen = false;
        let mut in_options = false;
        let mut in_references = false;
        let mut reference_start = None;
        for (index, token) in tokens.iter().enumerate() {
            match token.kind {
                TokenKind::LeftBrace => {
                    if in_references && object_depth == 1 && array_depth == 1 {
                        reference_start = Some(token.span.start);
                    }
                    object_depth += 1;
                }
                TokenKind::RightBrace => {
                    if in_references
                        && object_depth == 2
                        && array_depth == 1
                        && let Some(start) = reference_start.take()
                    {
                        spans
                            .references
                            .push((start, token.span.end.saturating_sub(start)));
                    }
                    if in_options && object_depth == 2 && array_depth == 0 {
                        in_options = false;
                    }
                    object_depth = object_depth.saturating_sub(1);
                }
                TokenKind::LeftBracket => array_depth += 1,
                TokenKind::RightBracket => {
                    if in_references && object_depth == 1 && array_depth == 1 {
                        in_references = false;
                    }
                    array_depth = array_depth.saturating_sub(1);
                }
                TokenKind::StringLiteral
                    if tokens
                        .get(index + 1)
                        .is_some_and(|token| token.kind == TokenKind::Colon) =>
                {
                    let Some(name) = json_token_string(source_text, token) else {
                        continue;
                    };
                    let value = tokens.get(index + 2);
                    if in_options && object_depth == 2 && array_depth == 0 {
                        if let Some((value, value_span)) =
                            value.and_then(|token| config_value(normalized, token))
                        {
                            spans.compiler_options.push(ConfigOptionOccurrence {
                                name,
                                value,
                                span: ConfigOptionSpans {
                                    key: (token.span.start, token.span.len()),
                                    value: Some(value_span),
                                },
                            });
                        }
                    } else if object_depth == 1 && array_depth == 0 {
                        if name == "compilerOptions" && spans.compiler_options_key.is_none() {
                            spans.compiler_options_key = Some((token.span.start, token.span.len()));
                        }
                        let value_kind = value.map(|token| token.kind);
                        if !options_seen
                            && name == "compilerOptions"
                            && value_kind == Some(TokenKind::LeftBrace)
                        {
                            options_seen = true;
                            in_options = true;
                        } else if !references_seen
                            && name == "references"
                            && value_kind == Some(TokenKind::LeftBracket)
                        {
                            references_seen = true;
                            in_references = true;
                        }
                    }
                }
                _ => {}
            }
        }
        spans
    }
}
fn config_value(normalized: &str, token: &Token) -> Option<(Value, (u32, u32))> {
    let start = token.span.start;
    let mut values = serde_json::Deserializer::from_str(&normalized[start as usize..]).into_iter();
    let value = values.next()?.ok()?;
    Some((value, (start, values.byte_offset() as u32)))
}
fn json_token_string(source_text: &str, token: &Token) -> Option<String> {
    (token.kind == TokenKind::StringLiteral)
        .then(|| &source_text[token.span.start as usize..token.span.end as usize])
        .and_then(|text| serde_json::from_str(text).ok())
}
fn normalize_jsonc(source_text: &str, tokens: &[Token]) -> String {
    let bytes = source_text.as_bytes();
    let mut normalized = vec![b' '; bytes.len()];
    for token in tokens {
        let range = token.span.start as usize..token.span.end as usize;
        normalized[range.clone()].copy_from_slice(&bytes[range]);
    }
    for pair in tokens.windows(2) {
        if pair[0].kind == TokenKind::Comma
            && matches!(
                pair[1].kind,
                TokenKind::RightBrace | TokenKind::RightBracket
            )
        {
            normalized[pair[0].span.start as usize..pair[0].span.end as usize].fill(b' ');
        }
    }
    String::from_utf8(normalized).expect("JSONC input started as UTF-8")
}
fn package_config_candidates(host: &dyn ProgramHost, package: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![package.to_path_buf()];
    if package.extension().is_none() {
        candidates.push(PathBuf::from(format!("{}.json", package.display())));
    }
    if host.directory_exists(package) {
        let package_json = package.join("package.json");
        if let Ok(text) = host.read_file(&package_json)
            && let Ok(document) = parse_jsonc(&text)
            && let Some(config) = document.value.get("tsconfig").and_then(Value::as_str)
        {
            candidates.push(absolute_path(package, Path::new(config)));
        }
        candidates.push(package.join(CONFIG_FILE_NAME));
    }
    candidates
}
fn discover_wildcard_files(
    host: &dyn ProgramHost,
    include: &Selector,
    exclude: &Selector,
    allow_js: bool,
    literal_files: &[PathBuf],
) -> Vec<PathBuf> {
    let mut literal_priorities: BTreeMap<String, u8> = BTreeMap::new();
    for (identity, priority) in literal_files
        .iter()
        .filter_map(|path| extension_identity(path, host.use_case_sensitive_file_names()))
    {
        literal_priorities
            .entry(identity)
            .and_modify(|current| *current = (*current).min(priority))
            .or_insert(priority);
    }
    let mut result: Vec<Option<PathBuf>> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut wildcard_priorities: BTreeMap<String, (u8, usize)> = BTreeMap::new();
    for raw in &include.values {
        let pattern = include_pattern(&include.origin, raw, host);
        let Some(base) = traversal_base(&pattern, host) else {
            continue;
        };
        let mut files = Vec::new();
        let mut visited = BTreeSet::new();
        visit_directory(host, &base, &pattern, &mut visited, &mut files);
        for file in files {
            if !supported_source_file(&file, allow_js)
                || !glob_matches(&pattern, &file, host.use_case_sensitive_file_names())
                || excluded_by(host, exclude, &file)
            {
                continue;
            }
            let key = path_key(&file, host.use_case_sensitive_file_names());
            if !seen.insert(key) {
                continue;
            }
            let Some((identity, priority)) =
                extension_identity(&file, host.use_case_sensitive_file_names())
            else {
                continue;
            };
            if literal_priorities
                .get(&identity)
                .is_some_and(|literal_priority| *literal_priority < priority)
            {
                continue;
            }
            if let Some((existing_priority, existing_index)) =
                wildcard_priorities.get(&identity).copied()
            {
                if existing_priority <= priority {
                    continue;
                }
                result[existing_index] = None;
            }
            let index = result.len();
            result.push(Some(file));
            wildcard_priorities.insert(identity, (priority, index));
        }
    }
    result.into_iter().flatten().collect()
}
fn extension_identity(path: &Path, case_sensitive: bool) -> Option<(String, u8)> {
    let normalized = display_path(path);
    let comparable = if case_sensitive {
        normalized
    } else {
        normalized.to_ascii_lowercase()
    };
    let groups = [
        [
            (".d.ts", 2),
            (".tsx", 1),
            (".ts", 0),
            (".jsx", 4),
            (".js", 3),
        ],
        [(".d.cts", 1), (".cts", 0), (".cjs", 2), ("", 0), ("", 0)],
        [(".d.mts", 1), (".mts", 0), (".mjs", 2), ("", 0), ("", 0)],
    ];
    for (group, extensions) in groups.iter().enumerate() {
        for (extension, priority) in extensions {
            if !extension.is_empty() && comparable.ends_with(extension) {
                let stem = &comparable[..comparable.len() - extension.len()];
                return Some((format!("{group}:{stem}"), *priority));
            }
        }
    }
    None
}
fn visit_directory(
    host: &dyn ProgramHost,
    directory: &Path,
    include_pattern: &str,
    visited: &mut BTreeSet<String>,
    files: &mut Vec<PathBuf>,
) {
    let real = host.realpath(directory);
    let key = path_key(&real, host.use_case_sensitive_file_names());
    if !visited.insert(key) {
        return;
    }
    let Ok(mut entries) = host.read_directory(directory) else {
        return;
    };
    entries.sort_by(|left, right| {
        left.path
            .file_name()
            .cmp(&right.path.file_name())
            .then_with(|| left.path.cmp(&right.path))
    });
    for entry in entries.iter().filter(|entry| entry.is_file) {
        files.push(normalize_path(&entry.path));
    }
    for entry in entries.iter().filter(|entry| entry.is_directory) {
        let name = entry
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if is_implicitly_excluded_directory(name)
            && !pattern_explicitly_includes_directory(
                include_pattern,
                name,
                host.use_case_sensitive_file_names(),
            )
        {
            continue;
        }
        visit_directory(host, &entry.path, include_pattern, visited, files);
    }
}
fn is_implicitly_excluded_directory(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "bower_components" | "jspm_packages")
}
fn pattern_explicitly_includes_directory(
    pattern: &str,
    directory_name: &str,
    case_sensitive: bool,
) -> bool {
    let (pattern, directory_name) = if case_sensitive {
        (pattern.to_string(), directory_name.to_string())
    } else {
        (
            pattern.to_ascii_lowercase(),
            directory_name.to_ascii_lowercase(),
        )
    };
    let package_directory = matches!(
        directory_name.as_str(),
        "node_modules" | "bower_components" | "jspm_packages"
    );
    pattern.split('/').any(|component| {
        if package_directory {
            component == directory_name
        } else {
            component.starts_with('.') && glob_segment(component, &directory_name)
        }
    })
}
fn include_pattern(origin: &Path, raw: &str, host: &dyn ProgramHost) -> String {
    let absolute = absolute_pattern(origin, raw);
    if !absolute.contains(['*', '?']) && host.directory_exists(Path::new(&absolute)) {
        format!("{}/**/*", absolute.trim_end_matches('/'))
    } else {
        absolute
    }
}
fn traversal_base(pattern: &str, host: &dyn ProgramHost) -> Option<PathBuf> {
    let path = Path::new(pattern);
    if !pattern.contains(['*', '?']) {
        if host.directory_exists(path) {
            return Some(path.to_path_buf());
        }
        return path.parent().map(Path::to_path_buf);
    }
    let mut base = PathBuf::new();
    for component in path.components() {
        if component
            .as_os_str()
            .to_string_lossy()
            .chars()
            .any(|character| matches!(character, '*' | '?'))
        {
            break;
        }
        base.push(component.as_os_str());
    }
    (!base.as_os_str().is_empty()).then_some(base)
}
fn excluded_by(host: &dyn ProgramHost, exclude: &Selector, file: &Path) -> bool {
    exclude.values.iter().any(|raw| {
        let pattern = absolute_pattern(&exclude.origin, raw);
        if pattern.contains(['*', '?']) {
            glob_matches(&pattern, file, host.use_case_sensitive_file_names())
        } else {
            let mut pattern = pattern.trim_end_matches('/').to_string();
            let mut file = display_path(file);
            if !host.use_case_sensitive_file_names() {
                pattern = pattern.to_ascii_lowercase();
                file = file.to_ascii_lowercase();
            }
            file == pattern || file.starts_with(&format!("{pattern}/"))
        }
    })
}
fn glob_matches(pattern: &str, file: &Path, case_sensitive: bool) -> bool {
    let (pattern_text, file_text) = if case_sensitive {
        (pattern.to_string(), display_path(file))
    } else {
        (
            pattern.to_ascii_lowercase(),
            display_path(file).to_ascii_lowercase(),
        )
    };
    let pattern_parts: Vec<&str> = pattern_text
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let file_parts: Vec<&str> = file_text
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    glob_parts(&pattern_parts, &file_parts)
}
fn glob_parts(pattern: &[&str], candidate: &[&str]) -> bool {
    match pattern.split_first() {
        None => candidate.is_empty(),
        Some((&"**", rest)) => {
            glob_parts(rest, candidate)
                || (!candidate.is_empty() && glob_parts(pattern, &candidate[1..]))
        }
        Some((segment, rest)) => {
            !candidate.is_empty()
                && glob_segment(segment, candidate[0])
                && glob_parts(rest, &candidate[1..])
        }
    }
}
fn glob_segment(pattern: &str, candidate: &str) -> bool {
    if candidate.starts_with('.') && !pattern.starts_with('.') && pattern.contains(['*', '?']) {
        return false;
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let candidate: Vec<char> = candidate.chars().collect();
    glob_segment_chars(&pattern, &candidate)
}
fn glob_segment_chars(pattern: &[char], candidate: &[char]) -> bool {
    match pattern.split_first() {
        None => candidate.is_empty(),
        Some((&'*', rest)) => {
            glob_segment_chars(rest, candidate)
                || (!candidate.is_empty() && glob_segment_chars(pattern, &candidate[1..]))
        }
        Some((&'?', rest)) => !candidate.is_empty() && glob_segment_chars(rest, &candidate[1..]),
        Some((&head, rest)) => {
            candidate.first() == Some(&head) && glob_segment_chars(rest, &candidate[1..])
        }
    }
}
fn supported_source_file(path: &Path, allow_js: bool) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name.ends_with(".ts")
        || name.ends_with(".tsx")
        || name.ends_with(".cts")
        || name.ends_with(".mts")
        || (allow_js
            && (name.ends_with(".js")
                || name.ends_with(".jsx")
                || name.ends_with(".cjs")
                || name.ends_with(".mjs")))
}
fn unsupported_root_message(display_name: &str, path: &Path, allow_js: bool) -> (u32, String) {
    if !allow_js && !supported_source_file(path, false) && supported_source_file(path, true) {
        return (
            6504,
            format!(
                "File '{display_name}' is a JavaScript file. Did you mean to enable the 'allowJs' option?"
            ),
        );
    }
    let extensions = if allow_js {
        "'.ts', '.tsx', '.d.ts', '.js', '.jsx', '.cts', '.d.cts', '.cjs', '.mts', '.d.mts', '.mjs'"
    } else {
        "'.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'"
    };
    (
        6054,
        format!(
            "File '{display_name}' has an unsupported extension. The only supported extensions are {extensions}."
        ),
    )
}
fn root_file_diagnostic(message_text: String, code: u32, reason: RootReason) -> Diagnostic {
    let (reason_text, reason_code) = reason.diagnostic();
    Diagnostic::global(message_text, code).with_related_information(vec![
        RelatedInformation::unlocated("The file is in the program because:", 1430, 1),
        RelatedInformation::unlocated(reason_text, reason_code, 2),
    ])
}
fn parse_jsonc(text: &str) -> Result<JsoncDocument, ()> {
    let source = SourceText::new(FileId(0), PathBuf::new(), Arc::from(text));
    let tokens = scan_source(&source).tokens;
    let normalized = normalize_jsonc(text, &tokens);
    let value = serde_json::from_str(normalized.trim_start_matches('\u{feff}')).map_err(|_| ())?;
    Ok(JsoncDocument {
        value,
        source_spans: ConfigSourceSpans::from_tokens(text, &normalized, &tokens),
    })
}
fn string_values(value: Option<&Value>, allow_scalar: bool) -> Option<Vec<String>> {
    match value {
        Some(Value::String(value)) if allow_scalar => Some(vec![value.clone()]),
        Some(Value::Array(values)) => Some(
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        ),
        _ => None,
    }
}
fn json_array(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}
fn absolute_pattern(origin: &Path, raw: &str) -> String {
    let normalized_raw = raw.replace('\\', "/");
    let path = Path::new(&normalized_raw);
    display_path(&absolute_path(origin, path))
}
fn absolute_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(&base.join(path))
    }
}
fn deduplicate_paths(paths: Vec<PathBuf>, case_sensitive: bool) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .map(|path| normalize_path(&path))
        .filter(|path| seen.insert(path_key(path, case_sensitive)))
        .collect()
}
fn path_key(path: &Path, case_sensitive: bool) -> String {
    if case_sensitive {
        display_path(path)
    } else {
        display_path(path).to_ascii_lowercase()
    }
}
fn logical_source_path_from_host(host: &dyn ProgramHost, path: &Path) -> PathBuf {
    let current_directory = normalize_path(host.current_directory());
    let path = normalize_path(path);
    if let Ok(relative) = path.strip_prefix(&current_directory) {
        // Preserve authored paths; realpath only resolves transport aliases such as `/var`.
        return relative.to_path_buf();
    }
    logical_path_from_host(&host.realpath(&current_directory), &host.realpath(&path))
}
fn logical_path_from_host(current_directory: &Path, path: &Path) -> PathBuf {
    let current_directory = normalize_path(current_directory);
    let path = normalize_path(path);
    if let Ok(relative) = path.strip_prefix(&current_directory) {
        return relative.to_path_buf();
    }
    let base_components: Vec<_> = current_directory.components().collect();
    let path_components: Vec<_> = path.components().collect();
    let common = base_components
        .iter()
        .zip(&path_components)
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return path;
    }
    let mut relative = PathBuf::new();
    for component in &base_components[common..] {
        if matches!(component, Component::Normal(_)) {
            relative.push("..");
        }
    }
    for component in &path_components[common..] {
        relative.push(component.as_os_str());
    }
    relative
}
#[cfg(test)]
#[path = "../rewrite-tests/config_jsonc_unit.rs"]
mod tests;
