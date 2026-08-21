//! TypeScript-compatible project selection and JSONC configuration loading.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::diagnostics::{Diagnostic, RelatedInformation, sort_and_deduplicate};
use crate::host::ProgramHost;
use crate::program::{CompilerOptions, SourceInput};
use crate::project_graph::{ProjectConfigId, ProjectGraph, ProjectReference};

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
    /// Discovery-affecting command-line override. Other explicit overrides are
    /// applied by the adapter after resolution.
    pub allow_js: Option<bool>,
    pub out_dir: Option<PathBuf>,
    pub declaration_dir: Option<PathBuf>,
}

impl ProjectRequest {
    #[must_use]
    pub const fn new(selection: ProjectSelection) -> Self {
        Self {
            selection,
            allow_js: None,
            out_dir: None,
            declaration_dir: None,
        }
    }

    #[must_use]
    pub const fn with_allow_js(mut self, allow_js: bool) -> Self {
        self.allow_js = Some(allow_js);
        self
    }

    #[must_use]
    pub fn with_out_dir(mut self, out_dir: impl Into<PathBuf>) -> Self {
        self.out_dir = Some(out_dir.into());
        self
    }

    #[must_use]
    pub fn with_declaration_dir(mut self, declaration_dir: impl Into<PathBuf>) -> Self {
        self.declaration_dir = Some(declaration_dir.into());
        self
    }
}

/// Fully resolved roots and configuration metadata, before parsing sources.
#[derive(Debug)]
pub struct ResolvedProject {
    pub options: CompilerOptions,
    pub inputs: Vec<SourceInput>,
    pub root_files: Vec<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    pub graph: ProjectGraph,
    pub(crate) provenance: ProjectProvenance,
}

/// A compiler option whose source can affect diagnostic ownership.
///
/// Process adapters clear an origin when a command-line value replaces the
/// configuration value. This keeps config-owned diagnostics located without
/// making provenance ambient compiler state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompilerOptionKey {
    Strict,
    NoImplicitAny,
    NoLib,
    Lib,
    AllowJs,
    NoCheck,
    NoEmit,
    NoEmitOnError,
    Declaration,
    DeclarationMap,
    SourceMap,
    InlineSourceMap,
    RemoveComments,
    Target,
    Module,
    RootDir,
    OutDir,
    DeclarationDir,
}

impl CompilerOptionKey {
    const ALL: [Self; 18] = [
        Self::Strict,
        Self::NoImplicitAny,
        Self::NoLib,
        Self::Lib,
        Self::AllowJs,
        Self::NoCheck,
        Self::NoEmit,
        Self::NoEmitOnError,
        Self::Declaration,
        Self::DeclarationMap,
        Self::SourceMap,
        Self::InlineSourceMap,
        Self::RemoveComments,
        Self::Target,
        Self::Module,
        Self::RootDir,
        Self::OutDir,
        Self::DeclarationDir,
    ];

    const fn json_name(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::NoImplicitAny => "noImplicitAny",
            Self::NoLib => "noLib",
            Self::Lib => "lib",
            Self::AllowJs => "allowJs",
            Self::NoCheck => "noCheck",
            Self::NoEmit => "noEmit",
            Self::NoEmitOnError => "noEmitOnError",
            Self::Declaration => "declaration",
            Self::DeclarationMap => "declarationMap",
            Self::SourceMap => "sourceMap",
            Self::InlineSourceMap => "inlineSourceMap",
            Self::RemoveComments => "removeComments",
            Self::Target => "target",
            Self::Module => "module",
            Self::RootDir => "rootDir",
            Self::OutDir => "outDir",
            Self::DeclarationDir => "declarationDir",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CompilerOptionOrigin {
    file: String,
    source_text: Arc<str>,
    key_start: u32,
    key_length: u32,
    value_start: Option<u32>,
    value_length: Option<u32>,
}

impl CompilerOptionOrigin {
    pub(crate) fn diagnostic_at_key(&self, message: String, code: u32) -> Diagnostic {
        Diagnostic::error_at_text(
            self.file.clone(),
            self.key_start,
            self.key_length,
            Arc::clone(&self.source_text),
            message,
            code,
        )
    }

    pub(crate) fn diagnostic_at_value(&self, message: String, code: u32) -> Diagnostic {
        Diagnostic::error_at_text(
            self.file.clone(),
            self.value_start.unwrap_or(self.key_start),
            self.value_length.unwrap_or(self.key_length),
            Arc::clone(&self.source_text),
            message,
            code,
        )
    }

    fn belongs_to(&self, config_path: &Path, current_directory: &Path) -> bool {
        self.file == display_path(&logical_path_from_host(current_directory, config_path))
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProjectProvenance {
    current_directory: PathBuf,
    entry_config_path: Option<PathBuf>,
    option_origins: BTreeMap<CompilerOptionKey, CompilerOptionOrigin>,
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

    pub(crate) fn entry_option_origin(
        &self,
        key: CompilerOptionKey,
    ) -> Option<&CompilerOptionOrigin> {
        let config_path = self.entry_config_path.as_deref()?;
        self.option_origin(key)
            .filter(|origin| origin.belongs_to(config_path, &self.current_directory))
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
    #[must_use]
    pub const fn root_file_count(&self) -> usize {
        self.root_files.len()
    }

    #[must_use]
    pub const fn source_file_count(&self) -> usize {
        self.inputs.len()
    }

    #[must_use]
    pub const fn project_config_count(&self) -> usize {
        self.graph.config_count()
    }

    #[must_use]
    pub fn project_reference_count(&self) -> usize {
        self.graph.reference_count()
    }

    /// Mark a configuration option as replaced by the process invocation.
    pub fn mark_command_line_option(&mut self, key: CompilerOptionKey) {
        self.provenance.option_origins.remove(&key);
    }
}

/// Resolve configuration inheritance, selectors, references, and source text.
///
/// Literal `files` roots are never filtered by `exclude`; wildcard roots are
/// grouped by include order and filesystem entries are compared
/// deterministically within each directory, matching TypeScript's ownership
/// and ordering rules.
#[must_use]
pub fn resolve_project(host: &dyn ProgramHost, request: &ProjectRequest) -> ResolvedProject {
    let mut resolver = Resolver::new(
        host,
        request.allow_js,
        request.out_dir.as_deref(),
        request.declaration_dir.as_deref(),
    );
    let (mut options, roots) = match &request.selection {
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
            (CompilerOptions::default(), roots)
        }
        ProjectSelection::Project(path) => resolver.resolve_explicit_project(path),
        ProjectSelection::Search(start) => resolver.resolve_searched_project(start),
    };
    if let Some(allow_js) = request.allow_js {
        options.allow_js = allow_js;
    }

    let root_files = deduplicate_paths(roots, host.use_case_sensitive_file_names());
    let mut inputs = Vec::with_capacity(root_files.len());
    for path in &root_files {
        let metadata = resolver.root_metadata(path);
        if !host.file_exists(path) {
            resolver.diagnostics.push(root_file_diagnostic(
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
                    resolver.diagnostics.push(root_file_diagnostic(
                        absolute_message,
                        code,
                        metadata.reason,
                    ));
                }
            }
            resolver
                .diagnostics
                .push(root_file_diagnostic(message, code, metadata.reason));
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
            Err(_) => resolver.diagnostics.push(root_file_diagnostic(
                format!("File '{}' not found.", metadata.display_path),
                6053,
                metadata.reason,
            )),
        }
    }
    sort_and_deduplicate(&mut resolver.diagnostics);
    let entry_config_path = resolver
        .graph
        .entry_config()
        .map(|config| config.path.clone());
    let root_reasons = resolver
        .roots
        .iter()
        .map(|(key, metadata)| (key.clone(), metadata.reason))
        .collect();
    let provenance = ProjectProvenance {
        current_directory: host.current_directory().to_path_buf(),
        entry_config_path,
        option_origins: resolver.option_origins,
        root_reasons,
        case_sensitive: host.use_case_sensitive_file_names(),
    };
    ResolvedProject {
        options,
        inputs,
        root_files,
        diagnostics: resolver.diagnostics,
        graph: resolver.graph,
        provenance,
    }
}

/// Search a directory and its ancestors for `tsconfig.json` without parsing
/// or expanding the project.
#[must_use]
pub fn find_config_file(host: &dyn ProgramHost, start: &Path) -> Option<PathBuf> {
    let absolute = absolute_path(host.current_directory(), start);
    let mut directory = if host.directory_exists(&absolute) {
        absolute
    } else {
        absolute.parent()?.to_path_buf()
    };
    loop {
        let candidate = directory.join(CONFIG_FILE_NAME);
        if host.file_exists(&candidate) {
            return Some(normalize_path(&candidate));
        }
        if !directory.pop() {
            return None;
        }
    }
}

struct Resolver<'a> {
    host: &'a dyn ProgramHost,
    allow_js_override: Option<bool>,
    out_dir_override: Option<PathBuf>,
    declaration_dir_override: Option<PathBuf>,
    diagnostics: Vec<Diagnostic>,
    graph: ProjectGraph,
    config_ids: BTreeMap<String, ProjectConfigId>,
    cache: BTreeMap<String, LoadedConfig>,
    incomplete_configs: BTreeSet<String>,
    roots: BTreeMap<String, RootMetadata>,
    option_origins: BTreeMap<CompilerOptionKey, CompilerOptionOrigin>,
}

impl<'a> Resolver<'a> {
    fn new(
        host: &'a dyn ProgramHost,
        allow_js_override: Option<bool>,
        out_dir_override: Option<&Path>,
        declaration_dir_override: Option<&Path>,
    ) -> Self {
        Self {
            host,
            allow_js_override,
            out_dir_override: out_dir_override
                .map(|path| absolute_path(host.current_directory(), path)),
            declaration_dir_override: declaration_dir_override
                .map(|path| absolute_path(host.current_directory(), path)),
            diagnostics: Vec::new(),
            graph: ProjectGraph::default(),
            config_ids: BTreeMap::new(),
            cache: BTreeMap::new(),
            incomplete_configs: BTreeSet::new(),
            roots: BTreeMap::new(),
            option_origins: BTreeMap::new(),
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
                logical_path: logical_path_from_host(self.host.current_directory(), host_path),
                reason: RootReason::CommandLine,
            })
    }

    fn resolve_explicit_project(&mut self, requested: &Path) -> (CompilerOptions, Vec<PathBuf>) {
        let absolute = absolute_path(self.host.current_directory(), requested);
        let config_path =
            if requested.as_os_str().is_empty() || self.host.directory_exists(&absolute) {
                let candidate = absolute.join(CONFIG_FILE_NAME);
                if !self.host.file_exists(&candidate) {
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

    fn resolve_searched_project(&mut self, start: &Path) -> (CompilerOptions, Vec<PathBuf>) {
        find_config_file(self.host, start).map_or_else(
            || (CompilerOptions::default(), Vec::new()),
            |candidate| self.resolve_config_entry(candidate),
        )
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
        let mut stack = Vec::new();
        let Some(loaded) = self.load_config(&config_path, &mut stack, true) else {
            return (CompilerOptions::default(), Vec::new());
        };
        self.graph.entry = Some(loaded.id);
        let mut options = CompilerOptions::default();
        loaded.merged.options.apply_to(&mut options);
        self.option_origins = loaded.merged.option_origins.clone();
        if let Some(allow_js) = self.allow_js_override {
            options.allow_js = allow_js;
            self.option_origins.remove(&CompilerOptionKey::AllowJs);
        }
        if let Some(out_dir) = &self.out_dir_override {
            options.out_dir = Some(out_dir.clone());
            self.option_origins.remove(&CompilerOptionKey::OutDir);
        }
        if let Some(declaration_dir) = &self.declaration_dir_override {
            options.declaration_dir = Some(declaration_dir.clone());
            self.option_origins
                .remove(&CompilerOptionKey::DeclarationDir);
        }
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
            if is_entry {
                self.graph.entry = Some(cached.id);
            }
            return Some(cached.clone());
        }
        let id = self.config_id(path.clone(), &key);
        if is_entry {
            self.graph.entry = Some(id);
        }
        let text: Arc<str> = match self.host.read_file(&path) {
            Ok(text) => Arc::from(text),
            Err(_) => {
                self.diagnostics.push(Diagnostic::global(
                    format!("Cannot read file '{}'.", display_path(&path)),
                    5083,
                ));
                return None;
            }
        };
        let value = match parse_jsonc(&text) {
            Ok(value) => value,
            Err(()) => {
                self.diagnostics.push(Diagnostic::global(
                    format!("Cannot read file '{}'.", display_path(&path)),
                    5083,
                ));
                return None;
            }
        };
        let object = value.as_object().cloned().unwrap_or_default();
        stack.push(path.clone());

        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        let extends_values = string_or_string_array(object.get("extends"));
        let mut merged = MergedConfig::default();
        let mut extends_ids = Vec::new();
        let mut bases_complete = true;
        for raw in &extends_values {
            let Some(extends_path) = self.resolve_extends_path(directory, raw) else {
                continue;
            };
            if let Some(base) = self.load_config(&extends_path, stack, false) {
                merged.merge_from(&base.merged);
                extends_ids.push(base.id);
                bases_complete &= base.complete;
            }
        }

        let own_options = partial_options(&object, directory);
        let own_origins = compiler_option_origins(
            &own_options,
            &text,
            &logical_path_from_host(self.host.current_directory(), &path),
        );
        merged.options.merge_from(&own_options);
        merged.option_origins.extend(own_origins);
        if let Some(values) = string_array_property(&object, "files") {
            merged.files = Some(Selector::new(values, directory));
        }
        if let Some(values) = string_array_property(&object, "include") {
            merged.include = Some(Selector::new(values, directory));
        }
        if let Some(values) = string_array_property(&object, "exclude") {
            merged.exclude = Some(Selector::new(values, directory));
        }
        merged.own_has_extends = object.contains_key("extends");
        merged.own_has_references = object.contains_key("references");

        let references = if is_entry {
            project_references(&object, &text, directory, id)
        } else {
            Vec::new()
        };
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
                self.diagnostics.push(Diagnostic::error_at_text(
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
        {
            let node = self.graph.config_mut(id);
            node.extends = extends_ids;
            node.references = references;
        }
        let loaded = LoadedConfig {
            id,
            merged,
            complete: bases_complete && !self.incomplete_configs.contains(&key),
        };
        if loaded.complete {
            self.cache.insert(key, loaded.clone());
        }
        Some(loaded)
    }

    fn config_id(&mut self, path: PathBuf, key: &str) -> ProjectConfigId {
        if let Some(id) = self.config_ids.get(key) {
            return *id;
        }
        let id = self.graph.add_config(path);
        self.config_ids.insert(key.to_string(), id);
        id
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
                    logical_path_from_host(self.host.current_directory(), &path),
                    RootReason::FilesList,
                );
                roots.push(path);
            }
        }

        let include = if loaded.merged.files.is_none() && loaded.merged.include.is_none() {
            Some(Selector::new(
                vec![DEFAULT_INCLUDE.to_string()],
                self.graph
                    .entry_config()
                    .and_then(|config| config.path.parent())
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

        let reference_count = self.graph.reference_count();
        if loaded
            .merged
            .files
            .as_ref()
            .is_some_and(|selector| selector.values.is_empty())
            && reference_count == 0
            && !loaded.merged.own_has_extends
        {
            let config = self.graph.entry_config().expect("entry config is set");
            self.diagnostics.push(Diagnostic::global(
                format!(
                    "The 'files' list in config file '{}' is empty.",
                    display_path(&config.path)
                ),
                18002,
            ));
        } else if roots.is_empty()
            && loaded.merged.files.is_none()
            && !loaded.merged.own_has_references
        {
            let config = self.graph.entry_config().expect("entry config is set");
            let include_values = include
                .as_ref()
                .map_or_else(Vec::new, |selector| selector.values.clone());
            self.diagnostics.push(Diagnostic::global(
                format!(
                    "No inputs were found in config file '{}'. Specified 'include' paths were '{}' and 'exclude' paths were '{}'.",
                    display_path(&config.path),
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
    id: ProjectConfigId,
    merged: MergedConfig,
    /// Only complete loads may enter the resolver cache. Cycle recovery can
    /// contribute partial options to this traversal without becoming a
    /// definitive answer for a later branch.
    complete: bool,
}

#[derive(Debug, Clone, Default)]
struct MergedConfig {
    options: PartialCompilerOptions,
    option_origins: BTreeMap<CompilerOptionKey, CompilerOptionOrigin>,
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

#[derive(Debug, Clone, Default)]
struct PartialCompilerOptions {
    strict: Option<bool>,
    no_implicit_any: Option<bool>,
    no_lib: Option<bool>,
    lib: Option<Vec<String>>,
    allow_js: Option<bool>,
    no_check: Option<bool>,
    no_emit: Option<bool>,
    no_emit_on_error: Option<bool>,
    declaration: Option<bool>,
    declaration_map: Option<bool>,
    source_map: Option<bool>,
    inline_source_map: Option<bool>,
    remove_comments: Option<bool>,
    target: Option<String>,
    module: Option<String>,
    root_dir: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    declaration_dir: Option<PathBuf>,
}

impl PartialCompilerOptions {
    const fn contains(&self, key: CompilerOptionKey) -> bool {
        match key {
            CompilerOptionKey::Strict => self.strict.is_some(),
            CompilerOptionKey::NoImplicitAny => self.no_implicit_any.is_some(),
            CompilerOptionKey::NoLib => self.no_lib.is_some(),
            CompilerOptionKey::Lib => self.lib.is_some(),
            CompilerOptionKey::AllowJs => self.allow_js.is_some(),
            CompilerOptionKey::NoCheck => self.no_check.is_some(),
            CompilerOptionKey::NoEmit => self.no_emit.is_some(),
            CompilerOptionKey::NoEmitOnError => self.no_emit_on_error.is_some(),
            CompilerOptionKey::Declaration => self.declaration.is_some(),
            CompilerOptionKey::DeclarationMap => self.declaration_map.is_some(),
            CompilerOptionKey::SourceMap => self.source_map.is_some(),
            CompilerOptionKey::InlineSourceMap => self.inline_source_map.is_some(),
            CompilerOptionKey::RemoveComments => self.remove_comments.is_some(),
            CompilerOptionKey::Target => self.target.is_some(),
            CompilerOptionKey::Module => self.module.is_some(),
            CompilerOptionKey::RootDir => self.root_dir.is_some(),
            CompilerOptionKey::OutDir => self.out_dir.is_some(),
            CompilerOptionKey::DeclarationDir => self.declaration_dir.is_some(),
        }
    }

    fn merge_from(&mut self, other: &Self) {
        macro_rules! replace_present {
            ($($field:ident),* $(,)?) => {
                $(if other.$field.is_some() { self.$field.clone_from(&other.$field); })*
            };
        }
        replace_present!(
            strict,
            no_implicit_any,
            no_lib,
            lib,
            allow_js,
            no_check,
            no_emit,
            no_emit_on_error,
            declaration,
            declaration_map,
            source_map,
            inline_source_map,
            remove_comments,
            target,
            module,
            root_dir,
            out_dir,
            declaration_dir,
        );
    }

    fn apply_to(&self, options: &mut CompilerOptions) {
        macro_rules! assign_copy {
            ($($field:ident),* $(,)?) => {
                $(if let Some(value) = self.$field { options.$field = value; })*
            };
        }
        assign_copy!(
            strict,
            no_lib,
            allow_js,
            no_check,
            no_emit,
            no_emit_on_error,
            declaration,
            declaration_map,
            source_map,
            inline_source_map,
            remove_comments,
        );
        if let Some(value) = self.no_implicit_any {
            options.no_implicit_any = Some(value);
        }
        if let Some(value) = &self.lib {
            options.lib = Some(value.clone());
        }
        if let Some(value) = &self.target {
            options.target.clone_from(value);
        }
        if let Some(value) = &self.module {
            options.module.clone_from(value);
        }
        if let Some(value) = &self.root_dir {
            options.root_dir = Some(value.clone());
        }
        if let Some(value) = &self.out_dir {
            options.out_dir = Some(value.clone());
        }
        if let Some(value) = &self.declaration_dir {
            options.declaration_dir = Some(value.clone());
        }
    }
}

fn partial_options(object: &Map<String, Value>, origin: &Path) -> PartialCompilerOptions {
    let Some(options) = object.get("compilerOptions").and_then(Value::as_object) else {
        return PartialCompilerOptions::default();
    };
    PartialCompilerOptions {
        strict: bool_property(options, "strict"),
        no_implicit_any: bool_property(options, "noImplicitAny"),
        no_lib: bool_property(options, "noLib"),
        lib: string_array_property(options, "lib"),
        allow_js: bool_property(options, "allowJs"),
        no_check: bool_property(options, "noCheck"),
        no_emit: bool_property(options, "noEmit"),
        no_emit_on_error: bool_property(options, "noEmitOnError"),
        declaration: bool_property(options, "declaration"),
        declaration_map: bool_property(options, "declarationMap"),
        source_map: bool_property(options, "sourceMap"),
        inline_source_map: bool_property(options, "inlineSourceMap"),
        remove_comments: bool_property(options, "removeComments"),
        target: string_property(options, "target"),
        module: string_property(options, "module"),
        root_dir: path_property(options, "rootDir", origin),
        out_dir: path_property(options, "outDir", origin),
        declaration_dir: path_property(options, "declarationDir", origin),
    }
}

fn compiler_option_origins(
    options: &PartialCompilerOptions,
    source_text: &Arc<str>,
    logical_path: &Path,
) -> BTreeMap<CompilerOptionKey, CompilerOptionOrigin> {
    let spans = compiler_option_spans(source_text);
    CompilerOptionKey::ALL
        .into_iter()
        .filter(|key| options.contains(*key))
        .filter_map(|key| {
            let span = spans.get(key.json_name())?;
            Some((
                key,
                CompilerOptionOrigin {
                    file: display_path(logical_path),
                    source_text: Arc::clone(source_text),
                    key_start: span.key_start,
                    key_length: span.key_length,
                    value_start: span.value_start,
                    value_length: span.value_length,
                },
            ))
        })
        .collect()
}

fn project_references(
    object: &Map<String, Value>,
    source_text: &str,
    origin: &Path,
    owner: ProjectConfigId,
) -> Vec<ProjectReference> {
    let spans = reference_object_spans(source_text);
    object
        .get("references")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .enumerate()
        .filter_map(|(index, reference)| {
            let raw = reference.get("path").and_then(Value::as_str)?;
            let (source_start, source_length) =
                spans.get(index).copied().flatten().unwrap_or((0, 0));
            Some(ProjectReference {
                owner,
                path: absolute_path(origin, Path::new(raw)),
                original_path: raw.to_string(),
                source_start,
                source_length,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum JsonTokenKind {
    String(String),
    Literal,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Colon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JsonToken {
    kind: JsonTokenKind,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConfigOptionSpans {
    key_start: u32,
    key_length: u32,
    value_start: Option<u32>,
    value_length: Option<u32>,
}

/// Locate direct properties of the top-level `compilerOptions` object.
///
/// The JSON value is parsed separately. This scanner owns only source
/// provenance, retaining the spelling and byte spans that diagnostics need.
fn compiler_option_spans(source_text: &str) -> BTreeMap<String, ConfigOptionSpans> {
    let tokens = jsonc_tokens(source_text);
    let mut object_depth = 0usize;
    let mut array_depth = 0usize;
    let mut options_open = None;
    for (index, token) in tokens.iter().enumerate() {
        match &token.kind {
            JsonTokenKind::LeftBrace => object_depth += 1,
            JsonTokenKind::RightBrace => object_depth = object_depth.saturating_sub(1),
            JsonTokenKind::LeftBracket => array_depth += 1,
            JsonTokenKind::RightBracket => array_depth = array_depth.saturating_sub(1),
            JsonTokenKind::String(value) if object_depth == 1 && array_depth == 0 => {
                if value == "compilerOptions"
                    && matches!(
                        tokens.get(index + 1).map(|token| &token.kind),
                        Some(JsonTokenKind::Colon)
                    )
                    && matches!(
                        tokens.get(index + 2).map(|token| &token.kind),
                        Some(JsonTokenKind::LeftBrace)
                    )
                {
                    options_open = Some(index + 2);
                    break;
                }
            }
            JsonTokenKind::String(_) | JsonTokenKind::Literal | JsonTokenKind::Colon => {}
        }
    }
    let Some(options_open) = options_open else {
        return BTreeMap::new();
    };

    let mut spans = BTreeMap::new();
    let mut nested_objects = 0usize;
    let mut nested_arrays = 0usize;
    let mut index = options_open + 1;
    while let Some(token) = tokens.get(index) {
        match &token.kind {
            JsonTokenKind::RightBrace if nested_objects == 0 && nested_arrays == 0 => break,
            JsonTokenKind::LeftBrace => nested_objects += 1,
            JsonTokenKind::RightBrace => nested_objects = nested_objects.saturating_sub(1),
            JsonTokenKind::LeftBracket => nested_arrays += 1,
            JsonTokenKind::RightBracket => nested_arrays = nested_arrays.saturating_sub(1),
            JsonTokenKind::String(name) if nested_objects == 0 && nested_arrays == 0 => {
                if matches!(
                    tokens.get(index + 1).map(|token| &token.kind),
                    Some(JsonTokenKind::Colon)
                ) {
                    let value = tokens.get(index + 2);
                    spans.insert(
                        name.clone(),
                        ConfigOptionSpans {
                            key_start: token.start as u32,
                            key_length: (token.end - token.start) as u32,
                            value_start: value.map(|value| value.start as u32),
                            value_length: value.map(|value| (value.end - value.start) as u32),
                        },
                    );
                }
            }
            JsonTokenKind::String(_) | JsonTokenKind::Literal | JsonTokenKind::Colon => {}
        }
        index += 1;
    }
    spans
}

/// Locate each object element of the top-level `references` array while
/// preserving byte offsets in the original JSONC source.
fn reference_object_spans(source_text: &str) -> Vec<Option<(u32, u32)>> {
    let tokens = jsonc_tokens(source_text);
    let mut object_depth = 0usize;
    let mut array_depth = 0usize;
    let mut references_open = None;
    for (index, token) in tokens.iter().enumerate() {
        match &token.kind {
            JsonTokenKind::LeftBrace => object_depth += 1,
            JsonTokenKind::RightBrace => object_depth = object_depth.saturating_sub(1),
            JsonTokenKind::LeftBracket => array_depth += 1,
            JsonTokenKind::RightBracket => array_depth = array_depth.saturating_sub(1),
            JsonTokenKind::String(value) if object_depth == 1 && array_depth == 0 => {
                if value == "references"
                    && matches!(
                        tokens.get(index + 1).map(|token| &token.kind),
                        Some(JsonTokenKind::Colon)
                    )
                    && matches!(
                        tokens.get(index + 2).map(|token| &token.kind),
                        Some(JsonTokenKind::LeftBracket)
                    )
                {
                    references_open = Some(index + 2);
                    break;
                }
            }
            JsonTokenKind::String(_) | JsonTokenKind::Literal | JsonTokenKind::Colon => {}
        }
    }
    let Some(references_open) = references_open else {
        return Vec::new();
    };

    let mut spans = Vec::new();
    let mut nested_arrays = 0usize;
    let mut nested_objects = 0usize;
    let mut object_start = None;
    for token in &tokens[references_open + 1..] {
        match token.kind {
            JsonTokenKind::LeftBracket => nested_arrays += 1,
            JsonTokenKind::RightBracket if nested_arrays > 0 => nested_arrays -= 1,
            JsonTokenKind::RightBracket if nested_objects == 0 => break,
            JsonTokenKind::LeftBrace if nested_objects == 0 && nested_arrays == 0 => {
                nested_objects = 1;
                object_start = Some(token.start);
            }
            JsonTokenKind::LeftBrace if nested_objects > 0 => nested_objects += 1,
            JsonTokenKind::RightBrace if nested_objects > 1 => nested_objects -= 1,
            JsonTokenKind::RightBrace if nested_objects == 1 => {
                nested_objects = 0;
                if let Some(start) = object_start.take() {
                    spans.push(Some((start as u32, (token.end - start) as u32)));
                }
            }
            JsonTokenKind::String(_)
            | JsonTokenKind::Literal
            | JsonTokenKind::Colon
            | JsonTokenKind::RightBrace
            | JsonTokenKind::LeftBrace
            | JsonTokenKind::RightBracket => {}
        }
    }
    spans
}

fn jsonc_tokens(source_text: &str) -> Vec<JsonToken> {
    let bytes = source_text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            index += 2;
            while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                index += 1;
            }
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            index += 2;
            while index + 1 < bytes.len() && bytes.get(index..index + 2) != Some(b"*/") {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        let start = index;
        let kind = match bytes[index] {
            b'{' => {
                index += 1;
                Some(JsonTokenKind::LeftBrace)
            }
            b'}' => {
                index += 1;
                Some(JsonTokenKind::RightBrace)
            }
            b'[' => {
                index += 1;
                Some(JsonTokenKind::LeftBracket)
            }
            b']' => {
                index += 1;
                Some(JsonTokenKind::RightBracket)
            }
            b':' => {
                index += 1;
                Some(JsonTokenKind::Colon)
            }
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' && index + 1 < bytes.len() {
                        index += 2;
                    } else if bytes[index] == b'"' {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
                serde_json::from_str(&source_text[start..index])
                    .ok()
                    .map(JsonTokenKind::String)
            }
            b',' => {
                index += 1;
                None
            }
            _ => {
                index += 1;
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && !matches!(bytes[index], b'{' | b'}' | b'[' | b']' | b':' | b',' | b'"')
                {
                    index += 1;
                }
                Some(JsonTokenKind::Literal)
            }
        };
        if let Some(kind) = kind {
            tokens.push(JsonToken {
                kind,
                start,
                end: index,
            });
        }
    }
    tokens
}

fn package_config_candidates(host: &dyn ProgramHost, package: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![package.to_path_buf()];
    if package.extension().is_none() {
        candidates.push(PathBuf::from(format!("{}.json", package.display())));
    }
    if host.directory_exists(package) {
        let package_json = package.join("package.json");
        if let Ok(text) = host.read_file(&package_json)
            && let Ok(value) = parse_jsonc(&text)
            && let Some(config) = value.get("tsconfig").and_then(Value::as_str)
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
            component.starts_with('.')
                && glob_segment(component.as_bytes(), directory_name.as_bytes())
        }
    })
}

fn include_pattern(origin: &Path, raw: &str, host: &dyn ProgramHost) -> String {
    let absolute = absolute_pattern(origin, raw);
    if !contains_wildcard(&absolute) && host.directory_exists(Path::new(&absolute)) {
        format!("{}/**/*", absolute.trim_end_matches('/'))
    } else {
        absolute
    }
}

fn traversal_base(pattern: &str, host: &dyn ProgramHost) -> Option<PathBuf> {
    let path = Path::new(pattern);
    if !contains_wildcard(pattern) {
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
    if base.as_os_str().is_empty() {
        return None;
    }
    Some(base)
}

fn excluded_by(host: &dyn ProgramHost, exclude: &Selector, file: &Path) -> bool {
    exclude.values.iter().any(|raw| {
        let pattern = absolute_pattern(&exclude.origin, raw);
        if contains_wildcard(&pattern) {
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
                && glob_segment(segment.as_bytes(), candidate[0].as_bytes())
                && glob_parts(rest, &candidate[1..])
        }
    }
}

fn glob_segment(pattern: &[u8], candidate: &[u8]) -> bool {
    let pattern = std::str::from_utf8(pattern).expect("glob pattern is UTF-8");
    let candidate = std::str::from_utf8(candidate).expect("source path is UTF-8");
    if candidate.starts_with('.') && !pattern.starts_with('.') && contains_wildcard(pattern) {
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
    if has_javascript_extension(path) && !allow_js {
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

fn has_javascript_extension(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name.ends_with(".js")
        || name.ends_with(".jsx")
        || name.ends_with(".cjs")
        || name.ends_with(".mjs")
}

fn root_file_diagnostic(message_text: String, code: u32, reason: RootReason) -> Diagnostic {
    let (reason_text, reason_code) = reason.diagnostic();
    Diagnostic::global(message_text, code).with_related_information(vec![
        RelatedInformation::unlocated("The file is in the program because:", 1430, 1),
        RelatedInformation::unlocated(reason_text, reason_code, 2),
    ])
}

fn parse_jsonc(text: &str) -> Result<Value, ()> {
    let uncommented = strip_json_comments(text.trim_start_matches('\u{feff}'));
    let normalized = strip_trailing_commas(&uncommented);
    serde_json::from_str(&normalized).map_err(|_| ())
}

fn strip_json_comments(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut in_string = false;
    while index < bytes.len() {
        if in_string {
            output.push(bytes[index]);
            if bytes[index] == b'\\' && index + 1 < bytes.len() {
                index += 1;
                output.push(bytes[index]);
            } else if bytes[index] == b'"' {
                in_string = false;
            }
            index += 1;
        } else if bytes[index] == b'"' {
            in_string = true;
            output.push(b'"');
            index += 1;
        } else if bytes.get(index..index + 2) == Some(b"//") {
            while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                output.push(b' ');
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            output.extend_from_slice(b"  ");
            index += 2;
            while index < bytes.len() && bytes.get(index..index + 2) != Some(b"*/") {
                output.push(if matches!(bytes[index], b'\n' | b'\r') {
                    bytes[index]
                } else {
                    b' '
                });
                index += 1;
            }
            if index < bytes.len() {
                output.extend_from_slice(b"  ");
                index = (index + 2).min(bytes.len());
            }
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).expect("JSONC input started as UTF-8")
}

fn strip_trailing_commas(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut in_string = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            in_string = !in_string;
            output.push(b'"');
            index += 1;
        } else if in_string && byte == b'\\' && index + 1 < bytes.len() {
            output.push(byte);
            index += 1;
            output.push(bytes[index]);
            index += 1;
        } else if !in_string && byte == b',' {
            let mut lookahead = index + 1;
            while bytes.get(lookahead).is_some_and(u8::is_ascii_whitespace) {
                lookahead += 1;
            }
            if matches!(bytes.get(lookahead), Some(b'}' | b']')) {
                output.push(b' ');
                index += 1;
            } else {
                output.push(b',');
                index += 1;
            }
        } else {
            output.push(byte);
            index += 1;
        }
    }
    String::from_utf8(output).expect("JSONC input started as UTF-8")
}

fn string_array_property(object: &Map<String, Value>, name: &str) -> Option<Vec<String>> {
    object.get(name).and_then(Value::as_array).map(|values| {
        values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

fn string_or_string_array(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn bool_property(object: &Map<String, Value>, name: &str) -> Option<bool> {
    object.get(name).and_then(Value::as_bool)
}

fn string_property(object: &Map<String, Value>, name: &str) -> Option<String> {
    object.get(name).and_then(Value::as_str).map(str::to_string)
}

fn path_property(object: &Map<String, Value>, name: &str, origin: &Path) -> Option<PathBuf> {
    string_property(object, name).map(|value| absolute_path(origin, Path::new(&value)))
}

fn json_array(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

fn contains_wildcard(path: &str) -> bool {
    path.contains(['*', '?'])
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

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
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
    let path = display_path(path);
    if case_sensitive {
        path
    } else {
        path.to_ascii_lowercase()
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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
