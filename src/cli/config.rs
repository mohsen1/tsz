use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Deserializer};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::path::{Path, PathBuf};

use crate::checker::context::ScriptTarget as CheckerScriptTarget;
use crate::emitter::{ModuleKind, PrinterOptions, ScriptTarget};

/// Custom deserializer for boolean options that accepts both bool and string values.
/// This handles cases where tsconfig.json contains `"strict": "true"` instead of `"strict": true`.
fn deserialize_bool_or_string<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    // Use a helper enum to deserialize either a bool or a string
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrString {
        Bool(bool),
        String(String),
    }

    match Option::<BoolOrString>::deserialize(deserializer)? {
        None => Ok(None),
        Some(BoolOrString::Bool(b)) => Ok(Some(b)),
        Some(BoolOrString::String(s)) => {
            // Parse common string representations of boolean values
            let normalized = s.trim().to_lowercase();
            match normalized.as_str() {
                "true" | "1" | "yes" | "on" => Ok(Some(true)),
                "false" | "0" | "no" | "off" => Ok(Some(false)),
                _ => {
                    // Invalid boolean string - return error with helpful message
                    Err(Error::custom(format!(
                        "invalid boolean value: '{}'. Expected true, false, 'true', or 'false'",
                        s
                    )))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TsConfig {
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub compiler_options: Option<CompilerOptions>,
    #[serde(default)]
    pub include: Option<Vec<String>>,
    #[serde(default)]
    pub exclude: Option<Vec<String>>,
    #[serde(default)]
    pub files: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompilerOptions {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub module_resolution: Option<String>,
    #[serde(default)]
    pub types_versions_compiler_version: Option<String>,
    #[serde(default)]
    pub types: Option<Vec<String>>,
    #[serde(default)]
    pub type_roots: Option<Vec<String>>,
    #[serde(default)]
    pub jsx: Option<String>,
    #[serde(default)]
    pub lib: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_lib: Option<bool>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub paths: Option<HashMap<String, Vec<String>>>,
    #[serde(default)]
    pub root_dir: Option<String>,
    #[serde(default)]
    pub out_dir: Option<String>,
    #[serde(default)]
    pub out_file: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub declaration: Option<bool>,
    #[serde(default)]
    pub declaration_dir: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub source_map: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub declaration_map: Option<bool>,
    #[serde(default)]
    pub ts_build_info_file: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub incremental: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub strict: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_emit: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_emit_on_error: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub isolated_modules: Option<bool>,
    /// Custom conditions for package.json exports resolution
    #[serde(default)]
    pub custom_conditions: Option<Vec<String>>,
    /// Emit additional JavaScript to ease support for importing CommonJS modules
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub es_module_interop: Option<bool>,
    /// Allow 'import x from y' when a module doesn't have a default export
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub allow_synthetic_default_imports: Option<bool>,
}

// Re-export CheckerOptions from checker::context for unified API
pub use crate::checker::context::CheckerOptions;

#[derive(Debug, Clone, Default)]
pub struct ResolvedCompilerOptions {
    pub printer: PrinterOptions,
    pub checker: CheckerOptions,
    pub jsx: Option<JsxEmit>,
    pub lib_files: Vec<PathBuf>,
    pub lib_is_default: bool,
    pub module_resolution: Option<ModuleResolutionKind>,
    pub types_versions_compiler_version: Option<String>,
    pub types: Option<Vec<String>>,
    pub type_roots: Option<Vec<PathBuf>>,
    pub base_url: Option<PathBuf>,
    pub paths: Option<Vec<PathMapping>>,
    pub root_dir: Option<PathBuf>,
    pub out_dir: Option<PathBuf>,
    pub out_file: Option<PathBuf>,
    pub declaration_dir: Option<PathBuf>,
    pub emit_declarations: bool,
    pub source_map: bool,
    pub declaration_map: bool,
    pub ts_build_info_file: Option<PathBuf>,
    pub incremental: bool,
    pub no_emit: bool,
    pub no_emit_on_error: bool,
    /// Custom conditions for package.json exports resolution
    pub custom_conditions: Vec<String>,
    /// Emit additional JavaScript to ease support for importing CommonJS modules
    pub es_module_interop: bool,
    /// Allow 'import x from y' when a module doesn't have a default export
    pub allow_synthetic_default_imports: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsxEmit {
    Preserve,
    ReactNative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleResolutionKind {
    Classic,
    Node,
    Node16,
    NodeNext,
    Bundler,
}

#[derive(Debug, Clone)]
pub struct PathMapping {
    pub(crate) pattern: String,
    pub(crate) prefix: String,
    pub(crate) suffix: String,
    pub(crate) targets: Vec<String>,
}

impl PathMapping {
    pub(crate) fn match_specifier(&self, specifier: &str) -> Option<String> {
        if !self.pattern.contains('*') {
            return if self.pattern == specifier {
                Some(String::new())
            } else {
                None
            };
        }

        if !specifier.starts_with(&self.prefix) || !specifier.ends_with(&self.suffix) {
            return None;
        }

        let start = self.prefix.len();
        let end = specifier.len().saturating_sub(self.suffix.len());
        if end < start {
            return None;
        }

        Some(specifier[start..end].to_string())
    }

    pub(crate) fn specificity(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }
}

impl ResolvedCompilerOptions {
    pub(crate) fn effective_module_resolution(&self) -> ModuleResolutionKind {
        if let Some(resolution) = self.module_resolution {
            return resolution;
        }

        match self.printer.module {
            ModuleKind::Node16 => ModuleResolutionKind::Node16,
            ModuleKind::NodeNext => ModuleResolutionKind::NodeNext,
            _ => ModuleResolutionKind::Node,
        }
    }
}

pub fn resolve_compiler_options(
    options: Option<&CompilerOptions>,
) -> Result<ResolvedCompilerOptions> {
    let mut resolved = ResolvedCompilerOptions::default();
    let Some(options) = options else {
        resolved.checker.target = checker_target_from_emitter(resolved.printer.target);
        resolved.lib_files = resolve_default_lib_files(resolved.printer.target)?;
        resolved.lib_is_default = true;
        return Ok(resolved);
    };

    if let Some(target) = options.target.as_deref() {
        resolved.printer.target = parse_script_target(target)?;
    }
    resolved.checker.target = checker_target_from_emitter(resolved.printer.target);

    if let Some(module) = options.module.as_deref() {
        resolved.printer.module = parse_module_kind(module)?;
    }

    if let Some(module_resolution) = options.module_resolution.as_deref() {
        let value = module_resolution.trim();
        if !value.is_empty() {
            resolved.module_resolution = Some(parse_module_resolution(value)?);
        }
    }

    if let Some(types_versions_compiler_version) =
        options.types_versions_compiler_version.as_deref()
    {
        let value = types_versions_compiler_version.trim();
        if !value.is_empty() {
            resolved.types_versions_compiler_version = Some(value.to_string());
        }
    }

    if let Some(types) = options.types.as_ref() {
        let list: Vec<String> = types
            .iter()
            .filter_map(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();
        resolved.types = Some(list);
    }

    if let Some(type_roots) = options.type_roots.as_ref() {
        let roots: Vec<PathBuf> = type_roots
            .iter()
            .filter_map(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                }
            })
            .collect();
        resolved.type_roots = Some(roots);
    }

    if let Some(jsx) = options.jsx.as_deref() {
        resolved.jsx = Some(parse_jsx_emit(jsx)?);
    }

    if let Some(no_lib) = options.no_lib {
        resolved.checker.no_lib = no_lib;
    }

    if let Some(lib_list) = options.lib.as_ref() {
        resolved.lib_files = resolve_lib_files(lib_list)?;
        resolved.lib_is_default = false;
    } else if !resolved.checker.no_lib {
        resolved.lib_files = resolve_default_lib_files(resolved.printer.target)?;
        resolved.lib_is_default = true;
    }

    let base_url = options.base_url.as_deref().map(str::trim);
    if let Some(base_url) = base_url
        && !base_url.is_empty()
    {
        resolved.base_url = Some(PathBuf::from(base_url));
    }

    if let Some(paths) = options.paths.as_ref() {
        let has_base_url = options
            .base_url
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        if !has_base_url {
            bail!("compilerOptions.paths requires compilerOptions.baseUrl");
        }
        if !paths.is_empty() {
            resolved.paths = Some(build_path_mappings(paths));
        }
    }

    if let Some(root_dir) = options.root_dir.as_deref()
        && !root_dir.is_empty()
    {
        resolved.root_dir = Some(PathBuf::from(root_dir));
    }

    if let Some(out_dir) = options.out_dir.as_deref()
        && !out_dir.is_empty()
    {
        resolved.out_dir = Some(PathBuf::from(out_dir));
    }

    if let Some(out_file) = options.out_file.as_deref()
        && !out_file.is_empty()
    {
        resolved.out_file = Some(PathBuf::from(out_file));
    }

    if let Some(declaration_dir) = options.declaration_dir.as_deref()
        && !declaration_dir.is_empty()
    {
        resolved.declaration_dir = Some(PathBuf::from(declaration_dir));
    }

    if let Some(declaration) = options.declaration {
        resolved.emit_declarations = declaration;
    }

    if let Some(source_map) = options.source_map {
        resolved.source_map = source_map;
    }

    if let Some(declaration_map) = options.declaration_map {
        resolved.declaration_map = declaration_map;
    }

    if let Some(ts_build_info_file) = options.ts_build_info_file.as_deref()
        && !ts_build_info_file.is_empty()
    {
        resolved.ts_build_info_file = Some(PathBuf::from(ts_build_info_file));
    }

    if let Some(incremental) = options.incremental {
        resolved.incremental = incremental;
    }

    if let Some(strict) = options.strict {
        resolved.checker.strict = strict;
        if strict {
            resolved.checker.no_implicit_any = true;
            resolved.checker.no_implicit_returns = true;
            resolved.checker.strict_null_checks = true;
            resolved.checker.strict_function_types = true;
            resolved.checker.strict_property_initialization = true;
            resolved.checker.no_implicit_this = true;
            resolved.checker.use_unknown_in_catch_variables = true;
        }
    }

    if let Some(no_emit) = options.no_emit {
        resolved.no_emit = no_emit;
    }

    if let Some(no_emit_on_error) = options.no_emit_on_error {
        resolved.no_emit_on_error = no_emit_on_error;
    }

    if let Some(isolated_modules) = options.isolated_modules {
        resolved.checker.isolated_modules = isolated_modules;
    }

    if let Some(ref custom_conditions) = options.custom_conditions {
        resolved.custom_conditions = custom_conditions.clone();
    }

    if let Some(es_module_interop) = options.es_module_interop {
        resolved.es_module_interop = es_module_interop;
        resolved.checker.es_module_interop = es_module_interop;
        // esModuleInterop implies allowSyntheticDefaultImports
        if es_module_interop {
            resolved.allow_synthetic_default_imports = true;
            resolved.checker.allow_synthetic_default_imports = true;
        }
    }

    if let Some(allow_synthetic_default_imports) = options.allow_synthetic_default_imports {
        resolved.allow_synthetic_default_imports = allow_synthetic_default_imports;
        resolved.checker.allow_synthetic_default_imports = allow_synthetic_default_imports;
    }

    Ok(resolved)
}

pub fn parse_tsconfig(source: &str) -> Result<TsConfig> {
    let stripped = strip_jsonc(source);
    let normalized = remove_trailing_commas(&stripped);
    let config = serde_json::from_str(&normalized).context("failed to parse tsconfig JSON")?;
    Ok(config)
}

pub fn load_tsconfig(path: &Path) -> Result<TsConfig> {
    let mut visited = HashSet::new();
    load_tsconfig_inner(path, &mut visited)
}

fn load_tsconfig_inner(path: &Path, visited: &mut HashSet<PathBuf>) -> Result<TsConfig> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        bail!("tsconfig extends cycle detected at {}", canonical.display());
    }

    let source = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read tsconfig: {}", path.display()))?;
    let mut config = parse_tsconfig(&source)
        .with_context(|| format!("failed to parse tsconfig: {}", path.display()))?;

    let extends = config.extends.take();
    if let Some(extends_path) = extends {
        let base_path = resolve_extends_path(path, &extends_path)?;
        let base_config = load_tsconfig_inner(&base_path, visited)?;
        config = merge_configs(base_config, config);
    }

    visited.remove(&canonical);
    Ok(config)
}

fn resolve_extends_path(current_path: &Path, extends: &str) -> Result<PathBuf> {
    let base_dir = current_path
        .parent()
        .ok_or_else(|| anyhow!("tsconfig has no parent directory"))?;
    let mut candidate = PathBuf::from(extends);
    if candidate.extension().is_none() {
        candidate.set_extension("json");
    }

    if candidate.is_absolute() {
        Ok(candidate)
    } else {
        Ok(base_dir.join(candidate))
    }
}

fn merge_configs(base: TsConfig, mut child: TsConfig) -> TsConfig {
    let merged_compiler_options = match (base.compiler_options, child.compiler_options.take()) {
        (Some(base_opts), Some(child_opts)) => Some(merge_compiler_options(base_opts, child_opts)),
        (Some(base_opts), None) => Some(base_opts),
        (None, Some(child_opts)) => Some(child_opts),
        (None, None) => None,
    };

    TsConfig {
        extends: None,
        compiler_options: merged_compiler_options,
        include: child.include.or(base.include),
        exclude: child.exclude.or(base.exclude),
        files: child.files.or(base.files),
    }
}

fn merge_compiler_options(base: CompilerOptions, child: CompilerOptions) -> CompilerOptions {
    CompilerOptions {
        target: child.target.or(base.target),
        module: child.module.or(base.module),
        module_resolution: child.module_resolution.or(base.module_resolution),
        types_versions_compiler_version: child
            .types_versions_compiler_version
            .or(base.types_versions_compiler_version),
        types: child.types.or(base.types),
        type_roots: child.type_roots.or(base.type_roots),
        jsx: child.jsx.or(base.jsx),
        lib: child.lib.or(base.lib),
        no_lib: child.no_lib.or(base.no_lib),
        base_url: child.base_url.or(base.base_url),
        paths: child.paths.or(base.paths),
        root_dir: child.root_dir.or(base.root_dir),
        out_dir: child.out_dir.or(base.out_dir),
        out_file: child.out_file.or(base.out_file),
        declaration: child.declaration.or(base.declaration),
        declaration_dir: child.declaration_dir.or(base.declaration_dir),
        source_map: child.source_map.or(base.source_map),
        declaration_map: child.declaration_map.or(base.declaration_map),
        ts_build_info_file: child.ts_build_info_file.or(base.ts_build_info_file),
        incremental: child.incremental.or(base.incremental),
        strict: child.strict.or(base.strict),
        no_emit: child.no_emit.or(base.no_emit),
        no_emit_on_error: child.no_emit_on_error.or(base.no_emit_on_error),
        isolated_modules: child.isolated_modules.or(base.isolated_modules),
        custom_conditions: child.custom_conditions.or(base.custom_conditions),
        es_module_interop: child.es_module_interop.or(base.es_module_interop),
        allow_synthetic_default_imports: child
            .allow_synthetic_default_imports
            .or(base.allow_synthetic_default_imports),
    }
}

fn parse_script_target(value: &str) -> Result<ScriptTarget> {
    let normalized = normalize_option(value);
    let target = match normalized.as_str() {
        "es3" => ScriptTarget::ES3,
        "es5" => ScriptTarget::ES5,
        "es6" | "es2015" => ScriptTarget::ES2015,
        "es2016" => ScriptTarget::ES2016,
        "es2017" => ScriptTarget::ES2017,
        "es2018" => ScriptTarget::ES2018,
        "es2019" => ScriptTarget::ES2019,
        "es2020" => ScriptTarget::ES2020,
        "es2021" => ScriptTarget::ES2021,
        "es2022" => ScriptTarget::ES2022,
        "esnext" => ScriptTarget::ESNext,
        _ => bail!("unsupported compilerOptions.target '{}'", value),
    };

    Ok(target)
}

fn parse_module_kind(value: &str) -> Result<ModuleKind> {
    let normalized = normalize_option(value);
    let module = match normalized.as_str() {
        "none" => ModuleKind::None,
        "commonjs" => ModuleKind::CommonJS,
        "amd" => ModuleKind::AMD,
        "umd" => ModuleKind::UMD,
        "system" => ModuleKind::System,
        "es6" | "es2015" => ModuleKind::ES2015,
        "es2020" => ModuleKind::ES2020,
        "es2022" => ModuleKind::ES2022,
        "esnext" => ModuleKind::ESNext,
        "node16" => ModuleKind::Node16,
        "nodenext" => ModuleKind::NodeNext,
        _ => bail!("unsupported compilerOptions.module '{}'", value),
    };

    Ok(module)
}

fn parse_module_resolution(value: &str) -> Result<ModuleResolutionKind> {
    let normalized = normalize_option(value);
    let resolution = match normalized.as_str() {
        "classic" => ModuleResolutionKind::Classic,
        "node" | "node10" => ModuleResolutionKind::Node,
        "node16" => ModuleResolutionKind::Node16,
        "nodenext" => ModuleResolutionKind::NodeNext,
        "bundler" => ModuleResolutionKind::Bundler,
        _ => bail!("unsupported compilerOptions.moduleResolution '{}'", value),
    };

    Ok(resolution)
}

fn parse_jsx_emit(value: &str) -> Result<JsxEmit> {
    let normalized = normalize_option(value);
    let jsx = match normalized.as_str() {
        "preserve" => JsxEmit::Preserve,
        "reactnative" => JsxEmit::ReactNative,
        _ => bail!("unsupported compilerOptions.jsx '{}'", value),
    };

    Ok(jsx)
}

fn build_path_mappings(paths: &HashMap<String, Vec<String>>) -> Vec<PathMapping> {
    let mut mappings = Vec::new();
    for (pattern, targets) in paths {
        if targets.is_empty() {
            continue;
        }
        let pattern = normalize_path_pattern(pattern);
        let targets = targets
            .iter()
            .map(|target| normalize_path_pattern(target))
            .collect();
        let (prefix, suffix) = split_path_pattern(&pattern);
        mappings.push(PathMapping {
            pattern,
            prefix,
            suffix,
            targets,
        });
    }
    mappings.sort_by(|left, right| {
        right
            .specificity()
            .cmp(&left.specificity())
            .then_with(|| right.pattern.len().cmp(&left.pattern.len()))
            .then_with(|| left.pattern.cmp(&right.pattern))
    });
    mappings
}

fn normalize_path_pattern(value: &str) -> String {
    value.trim().replace('\\', "/")
}

fn split_path_pattern(pattern: &str) -> (String, String) {
    match pattern.find('*') {
        Some(star_idx) => {
            let (prefix, rest) = pattern.split_at(star_idx);
            (prefix.to_string(), rest[1..].to_string())
        }
        None => (pattern.to_string(), String::new()),
    }
}

pub(crate) fn resolve_lib_files(lib_list: &[String]) -> Result<Vec<PathBuf>> {
    if lib_list.is_empty() {
        return Ok(Vec::new());
    }

    let lib_dir = default_lib_dir()?;
    let lib_map = build_lib_map(&lib_dir)?;
    let mut resolved = Vec::new();
    let mut pending: VecDeque<String> = lib_list
        .iter()
        .map(|value| normalize_lib_name(value))
        .collect();
    let mut visited = HashSet::new();

    while let Some(lib_name) = pending.pop_front() {
        if lib_name.is_empty() || !visited.insert(lib_name.clone()) {
            continue;
        }

        let path = match lib_map.get(&lib_name) {
            Some(path) => path.clone(),
            None => {
                let alias = match lib_name.as_str() {
                    "lib" => Some("es5"),
                    "es6" => Some("es2015"),
                    _ => None,
                };
                let Some(alias) = alias else {
                    return Err(anyhow!("unsupported compilerOptions.lib '{}'", lib_name));
                };
                lib_map
                    .get(alias)
                    .cloned()
                    .ok_or_else(|| anyhow!("unsupported compilerOptions.lib '{}'", lib_name))?
            }
        };
        resolved.push(path.clone());

        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read lib file {}", path.display()))?;
        for reference in extract_lib_references(&contents) {
            pending.push_back(reference);
        }
    }

    Ok(resolved)
}

pub(crate) fn resolve_default_lib_files(target: ScriptTarget) -> Result<Vec<PathBuf>> {
    let default_lib = default_lib_name_for_target(target);

    // Try to resolve from disk first
    match resolve_lib_files(&[default_lib.to_string()]) {
        Ok(files) => return Ok(files),
        Err(_) => {} // Fall through to fallbacks
    };

    let mut fallbacks = Vec::new();
    if default_lib == "lib" {
        fallbacks.push("es5");
    }
    if default_lib == "es6" {
        fallbacks.push("es2015");
    }
    if default_lib != "lib" {
        fallbacks.push("lib");
    }

    for fallback in &fallbacks {
        if let Ok(files) = resolve_lib_files(&[fallback.to_string()]) {
            return Ok(files);
        }
    }

    // If all disk-based resolution fails, return lib file paths anyway
    // (even though they don't exist on disk). The loader will use embedded libs
    // as a fallback via get_lib_by_file_name().
    // This is critical for conformance tests where lib files aren't on disk.
    let lib_dir = default_lib_dir().unwrap_or_else(|_| PathBuf::from("lib"));
    let lib_file = lib_dir.join(format!("{}.d.ts", default_lib));

    // Also include fallback lib files for better coverage
    let mut result = vec![lib_file];

    for fallback in &fallbacks {
        let fallback_file = lib_dir.join(format!("{}.d.ts", fallback));
        result.push(fallback_file);
    }

    Ok(result)
}

fn default_lib_name_for_target(target: ScriptTarget) -> &'static str {
    match target {
        ScriptTarget::ES3 | ScriptTarget::ES5 => "lib",
        ScriptTarget::ES2015 => "es6",
        ScriptTarget::ES2016 => "es2016.full",
        ScriptTarget::ES2017 => "es2017.full",
        ScriptTarget::ES2018 => "es2018.full",
        ScriptTarget::ES2019 => "es2019.full",
        ScriptTarget::ES2020 => "es2020.full",
        ScriptTarget::ES2021 => "es2021.full",
        ScriptTarget::ES2022 => "es2022.full",
        ScriptTarget::ESNext => "esnext.full",
    }
}

fn default_lib_dir() -> Result<PathBuf> {
    if let Some(dir) = lib_dir_from_env() {
        return Ok(dir);
    }

    if let Some(dir) = lib_dir_from_exe() {
        return Ok(dir);
    }

    if let Some(dir) = lib_dir_from_cwd() {
        return Ok(dir);
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    if let Some(dir) = lib_dir_from_root(manifest_dir) {
        return Ok(dir);
    }

    bail!("lib directory not found under {}", manifest_dir.display());
}

fn lib_dir_from_env() -> Option<PathBuf> {
    let dir = env::var_os("TSZ_LIB_DIR")?;
    let dir = PathBuf::from(dir);
    if dir.is_dir() {
        Some(canonicalize_or_owned(&dir))
    } else {
        None
    }
}

fn lib_dir_from_exe() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidate = exe_dir.join("lib");
    if candidate.is_dir() {
        return Some(canonicalize_or_owned(&candidate));
    }
    lib_dir_from_root(exe_dir)
}

fn lib_dir_from_cwd() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    lib_dir_from_root(&cwd)
}

fn lib_dir_from_root(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.join("TypeScript").join("lib"),
        root.join("TypeScript").join("src").join("lib"),
        root.join("TypeScript")
            .join("node_modules")
            .join("typescript")
            .join("lib"),
        root.join("tests").join("lib"),
    ];

    for candidate in candidates {
        if candidate.is_dir() {
            return Some(canonicalize_or_owned(&candidate));
        }
    }

    None
}

fn build_lib_map(lib_dir: &Path) -> Result<HashMap<String, PathBuf>> {
    let mut map = HashMap::new();
    for entry in std::fs::read_dir(lib_dir)
        .with_context(|| format!("failed to read lib directory {}", lib_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".d.ts") {
            continue;
        }

        let stem = file_name.trim_end_matches(".d.ts");
        let stem = stem.strip_suffix(".generated").unwrap_or(stem);
        let key = normalize_lib_name(stem);
        map.insert(key, canonicalize_or_owned(&path));
    }

    Ok(map)
}

/// Extract /// <reference lib="..." /> directives from a lib file source.
/// Returns a list of referenced lib names.
pub(crate) fn extract_lib_references(source: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut in_block_comment = false;
    for line in source.lines() {
        let line = line.trim_start();
        if in_block_comment {
            if line.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if line.starts_with("/*") {
            if !line.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if line.starts_with("///") {
            if let Some(value) = parse_reference_lib_value(line) {
                refs.push(normalize_lib_name(value));
            }
            continue;
        }
        if line.starts_with("//") {
            continue;
        }
        break;
    }
    refs
}

fn parse_reference_lib_value(line: &str) -> Option<&str> {
    let mut offset = 0;
    let bytes = line.as_bytes();
    while let Some(idx) = line[offset..].find("lib=") {
        let start = offset + idx;
        if start > 0 {
            let prev = bytes[start - 1];
            if !prev.is_ascii_whitespace() && prev != b'<' {
                offset = start + 4;
                continue;
            }
        }
        let quote = *bytes.get(start + 4)?;
        if quote != b'"' && quote != b'\'' {
            offset = start + 4;
            continue;
        }
        let rest = &line[start + 5..];
        let end = rest.find(quote as char)?;
        return Some(&rest[..end]);
    }
    None
}

fn normalize_lib_name(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    normalized
        .strip_prefix("lib.")
        .unwrap_or(normalized.as_str())
        .to_string()
}

pub(crate) fn checker_target_from_emitter(target: ScriptTarget) -> CheckerScriptTarget {
    match target {
        ScriptTarget::ES3 => CheckerScriptTarget::ES3,
        ScriptTarget::ES5 => CheckerScriptTarget::ES5,
        ScriptTarget::ES2015 => CheckerScriptTarget::ES2015,
        ScriptTarget::ES2016 => CheckerScriptTarget::ES2016,
        ScriptTarget::ES2017 => CheckerScriptTarget::ES2017,
        ScriptTarget::ES2018 => CheckerScriptTarget::ES2018,
        ScriptTarget::ES2019 => CheckerScriptTarget::ES2019,
        ScriptTarget::ES2020 => CheckerScriptTarget::ES2020,
        ScriptTarget::ES2021 | ScriptTarget::ES2022 | ScriptTarget::ESNext => {
            CheckerScriptTarget::ESNext
        }
    }
}

fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn normalize_option(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch == '-' || ch == '_' || ch.is_whitespace() {
            continue;
        }
        normalized.push(ch.to_ascii_lowercase());
    }
    normalized
}

fn strip_jsonc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                out.push(ch);
            }
            continue;
        }

        if in_block_comment {
            if ch == '*' {
                if let Some('/') = chars.peek().copied() {
                    chars.next();
                    in_block_comment = false;
                }
            } else if ch == '\n' {
                out.push(ch);
            }
            continue;
        }

        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == '/'
            && let Some(&next) = chars.peek()
        {
            if next == '/' {
                chars.next();
                in_line_comment = true;
                continue;
            }
            if next == '*' {
                chars.next();
                in_block_comment = true;
                continue;
            }
        }

        out.push(ch);
    }

    out
}

fn remove_trailing_commas(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            while let Some(next) = lookahead.peek().copied() {
                if next.is_whitespace() {
                    lookahead.next();
                    continue;
                }
                if next == '}' || next == ']' {
                    break;
                }
                break;
            }

            if let Some(next) = lookahead.peek().copied()
                && (next == '}' || next == ']')
            {
                continue;
            }
        }

        out.push(ch);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_boolean_true() {
        let json = r#"{"strict": true}"#;
        let opts: CompilerOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.strict, Some(true));
    }

    #[test]
    fn test_parse_string_true() {
        let json = r#"{"strict": "true"}"#;
        let opts: CompilerOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.strict, Some(true));
    }

    #[test]
    fn test_parse_invalid_string() {
        let json = r#"{"strict": "invalid"}"#;
        let result: Result<CompilerOptions, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
