use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Deserializer};
use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};
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
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_types_and_symbols: Option<bool>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub paths: Option<FxHashMap<String, Vec<String>>>,
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
    /// Enable experimental support for legacy experimental decorators
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub experimental_decorators: Option<bool>,
    /// Allow JavaScript files to be a part of your program
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub allow_js: Option<bool>,
    /// Enable error reporting in type-checked JavaScript files
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub check_js: Option<bool>,
    /// Parse in strict mode and emit "use strict" for each source file
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub always_strict: Option<bool>,
    /// Report errors on unused local variables
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_unused_locals: Option<bool>,
    /// Report errors on unused parameters
    #[serde(default, deserialize_with = "deserialize_bool_or_string")]
    pub no_unused_parameters: Option<bool>,
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
    /// Disable full type checking (only parse and emit errors reported).
    pub no_check: bool,
    /// Custom conditions for package.json exports resolution
    pub custom_conditions: Vec<String>,
    /// Emit additional JavaScript to ease support for importing CommonJS modules
    pub es_module_interop: bool,
    /// Allow 'import x from y' when a module doesn't have a default export
    pub allow_synthetic_default_imports: bool,
    /// Allow JavaScript files to be part of the program
    pub allow_js: bool,
    /// Enable error reporting in type-checked JavaScript files
    pub check_js: bool,
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
        let kind = parse_module_kind(module)?;
        resolved.printer.module = kind;
        resolved.checker.module = kind;
    } else {
        // Default to CommonJS if not specified (matches tsc behavior)
        // Note: tsc only changes the default module kind when 'module' is explicitly set
        // The target does NOT affect the default module kind
        resolved.printer.module = ModuleKind::CommonJS;
        resolved.checker.module = ModuleKind::CommonJS;
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

    if let Some(no_types_and_symbols) = options.no_types_and_symbols {
        resolved.checker.no_types_and_symbols = no_types_and_symbols;
    }

    if resolved.checker.no_lib && options.lib.is_some() {
        bail!("Option 'lib' cannot be specified with option 'noLib'.");
    }

    if let Some(lib_list) = options.lib.as_ref() {
        resolved.lib_files = resolve_lib_files(lib_list)?;
        resolved.lib_is_default = false;
    } else if !resolved.checker.no_lib && !resolved.checker.no_types_and_symbols {
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
            resolved.checker.always_strict = true;
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

    if let Some(always_strict) = options.always_strict {
        resolved.checker.always_strict = always_strict;
    }

    if let Some(no_unused_locals) = options.no_unused_locals {
        resolved.checker.no_unused_locals = no_unused_locals;
    }

    if let Some(no_unused_parameters) = options.no_unused_parameters {
        resolved.checker.no_unused_parameters = no_unused_parameters;
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
    } else if !resolved.allow_synthetic_default_imports {
        // TypeScript defaults allowSyntheticDefaultImports to true unconditionally.
        // Only skip if it was already set by esModuleInterop above.
        resolved.allow_synthetic_default_imports = true;
        resolved.checker.allow_synthetic_default_imports = true;
    }

    if let Some(experimental_decorators) = options.experimental_decorators {
        resolved.checker.experimental_decorators = experimental_decorators;
    }

    if let Some(allow_js) = options.allow_js {
        resolved.allow_js = allow_js;
    }

    if let Some(check_js) = options.check_js {
        resolved.check_js = check_js;
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
    let mut visited = FxHashSet::default();
    load_tsconfig_inner(path, &mut visited)
}

fn load_tsconfig_inner(path: &Path, visited: &mut FxHashSet<PathBuf>) -> Result<TsConfig> {
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
        no_types_and_symbols: child.no_types_and_symbols.or(base.no_types_and_symbols),
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
        experimental_decorators: child
            .experimental_decorators
            .or(base.experimental_decorators),
        allow_js: child.allow_js.or(base.allow_js),
        check_js: child.check_js.or(base.check_js),
        always_strict: child.always_strict.or(base.always_strict),
        no_unused_locals: child.no_unused_locals.or(base.no_unused_locals),
        no_unused_parameters: child.no_unused_parameters.or(base.no_unused_parameters),
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

fn build_path_mappings(paths: &FxHashMap<String, Vec<String>>) -> Vec<PathMapping> {
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

/// Resolve lib files from names, optionally following `/// <reference lib="..." />` directives.
///
/// When `follow_references` is true, each lib file is scanned for reference directives
/// and those referenced libs are also loaded. When false, only the explicitly listed
/// libs are loaded without following their internal references.
///
/// TypeScript always follows `/// <reference lib="..." />` directives when loading libs.
/// For example, `lib.dom.d.ts` references `es2015` and `es2018.asynciterable`, so even
/// `--target es5` (which loads lib.d.ts -> dom) transitively loads ES2015 features.
/// Verified with `tsc 6.0.0-dev --target es5 --listFiles`.
pub(crate) fn resolve_lib_files_with_options(
    lib_list: &[String],
    follow_references: bool,
) -> Result<Vec<PathBuf>> {
    if lib_list.is_empty() {
        return Ok(Vec::new());
    }

    let lib_dir = default_lib_dir()?;
    resolve_lib_files_from_dir_with_options(lib_list, follow_references, &lib_dir)
}

pub(crate) fn resolve_lib_files_from_dir_with_options(
    lib_list: &[String],
    follow_references: bool,
    lib_dir: &Path,
) -> Result<Vec<PathBuf>> {
    if lib_list.is_empty() {
        return Ok(Vec::new());
    }

    let lib_map = build_lib_map(&lib_dir)?;
    let mut resolved = Vec::new();
    let mut pending: VecDeque<String> = lib_list
        .iter()
        .map(|value| normalize_lib_name(value))
        .collect();
    let mut visited = FxHashSet::default();

    while let Some(lib_name) = pending.pop_front() {
        if lib_name.is_empty() || !visited.insert(lib_name.clone()) {
            continue;
        }

        let path = match lib_map.get(&lib_name) {
            Some(path) => path.clone(),
            None => {
                // Handle tsc compatibility aliases:
                // - "lib" refers to lib.d.ts which is equivalent to es5.full.d.ts
                // - "es6" refers to lib.es6.d.ts which is equivalent to es2015.full.d.ts
                // - "es7" refers to lib.es2016.d.ts which is equivalent to es2016.d.ts
                let alias = match lib_name.as_str() {
                    "lib" => Some("es5.full"),
                    "es6" => Some("es2015.full"),
                    "es7" => Some("es2016"),
                    _ => None,
                };
                let Some(alias) = alias else {
                    return Err(anyhow!(
                        "unsupported compilerOptions.lib '{}' (not found in {})",
                        lib_name,
                        lib_dir.display()
                    ));
                };
                lib_map.get(alias).cloned().ok_or_else(|| {
                    anyhow!(
                        "unsupported compilerOptions.lib '{}' (alias '{}' not found in {})",
                        lib_name,
                        alias,
                        lib_dir.display()
                    )
                })?
            }
        };
        resolved.push(path.clone());

        // Only follow /// <reference lib="..." /> directives if requested
        if follow_references {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read lib file {}", path.display()))?;
            for reference in extract_lib_references(&contents) {
                pending.push_back(reference);
            }
        }
    }

    Ok(resolved)
}

/// Resolve lib files from names, following `/// <reference lib="..." />` directives.
/// This is used when explicitly specifying libs via `--lib`.
pub(crate) fn resolve_lib_files(lib_list: &[String]) -> Result<Vec<PathBuf>> {
    resolve_lib_files_with_options(lib_list, true)
}

pub(crate) fn resolve_lib_files_from_dir(
    lib_list: &[String],
    lib_dir: &Path,
) -> Result<Vec<PathBuf>> {
    resolve_lib_files_from_dir_with_options(lib_list, true, lib_dir)
}

/// Resolve default lib files for a given target.
///
/// Matches tsc's behavior exactly:
/// 1. Get the root lib file for the target (e.g., "lib" for ES5, "es6" for ES2015)
/// 2. Follow ALL `/// <reference lib="..." />` directives recursively
///
/// This means `--target es5` loads lib.d.ts -> dom -> es2015 (transitively),
/// which is exactly what tsc does (verified with `tsc --target es5 --listFiles`).
pub(crate) fn resolve_default_lib_files(target: ScriptTarget) -> Result<Vec<PathBuf>> {
    let lib_dir = default_lib_dir()?;
    resolve_default_lib_files_from_dir(target, &lib_dir)
}

pub(crate) fn resolve_default_lib_files_from_dir(
    target: ScriptTarget,
    lib_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let root_lib = default_lib_name_for_target(target);
    resolve_lib_files_from_dir(&[root_lib.to_string()], lib_dir)
}

/// Get the default lib name for a target.
///
/// This matches tsc's default behavior exactly:
/// - Each target loads the corresponding `.full` lib which includes:
///   - The ES version libs (e.g., es5, es2015.promise, etc.)
///   - DOM types (document, window, console, fetch, etc.)
///   - ScriptHost types
///
/// The mapping matches TypeScript's `getDefaultLibFileName()` in utilitiesPublic.ts:
/// - ES3/ES5 → lib.d.ts (equivalent to es5.full.d.ts in source tree)
/// - ES2015  → lib.es6.d.ts (equivalent to es2015.full.d.ts in source tree)
/// - ES2016+ → lib.es20XX.full.d.ts
/// - ESNext  → lib.esnext.full.d.ts
///
/// Note: The source tree uses `es5.full.d.ts` naming, while built TypeScript uses `lib.d.ts`.
/// We use the source tree naming since that's what exists in TypeScript/src/lib.
pub fn default_lib_name_for_target(target: ScriptTarget) -> &'static str {
    match target {
        // ES3/ES5 -> lib.d.ts (ES5 + DOM + ScriptHost)
        ScriptTarget::ES3 | ScriptTarget::ES5 => "lib",
        // ES2015 -> lib.es6.d.ts (ES2015 + DOM + DOM.Iterable + ScriptHost)
        // Note: NOT "es2015.full" (doesn't exist), use "es6" per tsc convention
        ScriptTarget::ES2015 => "es6",
        // ES2016+ use .full variants (ES + DOM + ScriptHost + others)
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

/// Get the core lib name for a target (without DOM/ScriptHost).
///
/// This is useful for conformance testing where:
/// 1. Tests don't need DOM types
/// 2. Core libs are smaller and faster to load
/// 3. Tests that need DOM should specify @lib: dom explicitly
pub fn core_lib_name_for_target(target: ScriptTarget) -> &'static str {
    match target {
        ScriptTarget::ES3 | ScriptTarget::ES5 => "es5",
        ScriptTarget::ES2015 => "es2015",
        ScriptTarget::ES2016 => "es2016",
        ScriptTarget::ES2017 => "es2017",
        ScriptTarget::ES2018 => "es2018",
        ScriptTarget::ES2019 => "es2019",
        ScriptTarget::ES2020 => "es2020",
        ScriptTarget::ES2021 => "es2021",
        ScriptTarget::ES2022 => "es2022",
        ScriptTarget::ESNext => "esnext",
    }
}

/// Get the default lib directory.
///
/// Searches in order:
/// 1. TSZ_LIB_DIR environment variable
/// 2. Relative to the executable
/// 3. Relative to current working directory
/// 4. TypeScript/src/lib in the source tree
pub fn default_lib_dir() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("TSZ_LIB_DIR") {
        let dir = PathBuf::from(dir);
        if !dir.is_dir() {
            bail!(
                "TSZ_LIB_DIR does not point to a directory: {}",
                dir.display()
            );
        }
        return Ok(canonicalize_or_owned(&dir));
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
        // Built/compiled libs from tsc build output (highest priority)
        root.join("TypeScript").join("built").join("local"),
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

fn build_lib_map(lib_dir: &Path) -> Result<FxHashMap<String, PathBuf>> {
    let mut map = FxHashMap::default();
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

/// Convert emitter ScriptTarget to checker ScriptTarget.
/// The emitter has more variants (ES2021, ES2022) which map to ESNext in the checker.
pub fn checker_target_from_emitter(target: ScriptTarget) -> CheckerScriptTarget {
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
