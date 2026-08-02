use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

use crate::checker::context::CheckerOptions;
use crate::emitter::{ModuleKind, PrinterOptions, ScriptTarget};
use crate::module_resolver_helpers::match_prefix_suffix;
use tsz_common::options::module_detection::ModuleDetectionKind;
use tsz_common::options::strict_family::{StrictFamilyOverrides, apply_strict_family};

use super::{
    CompilerOptions, build_path_mappings, checker_target_from_emitter,
    is_valid_identifier_or_qualified_name, jsx_emit_to_mode, normalize_option, parse_jsx_emit,
    parse_module_kind, parse_module_resolution, parse_new_line_kind, parse_script_target,
    resolve_default_lib_files, resolve_lib_files,
};

#[derive(Debug, Clone, Default)]
pub struct ResolvedCompilerOptions {
    pub printer: PrinterOptions,
    pub checker: CheckerOptions,
    pub jsx: Option<JsxEmit>,
    pub lib_files: Vec<PathBuf>,
    pub lib_is_default: bool,
    pub lib_replacement: bool,
    pub module_resolution: Option<ModuleResolutionKind>,
    pub resolve_package_json_exports: bool,
    pub resolve_package_json_imports: bool,
    pub module_suffixes: Vec<String>,
    pub resolve_json_module: bool,
    pub allow_arbitrary_extensions: bool,
    pub allow_importing_ts_extensions: bool,
    pub rewrite_relative_import_extensions: bool,
    pub trace_resolution: bool,
    pub types_versions_compiler_version: Option<String>,
    pub types: Option<Vec<String>>,
    pub type_roots: Option<Vec<PathBuf>>,
    pub base_url: Option<PathBuf>,
    pub paths: Option<Vec<PathMapping>>,
    /// Base directory for `paths` substitutions when `baseUrl` is not set
    /// (tsc's `pathsBasePath`). Since TypeScript 4.1 `paths` may be configured
    /// without `baseUrl`; relative substitutions then resolve against the
    /// directory of the tsconfig that declared them. tsc's
    /// `getPathsBasePath` returns `baseUrl ?? pathsBasePath`, so the resolver
    /// prefers `base_url` and falls back to this when `baseUrl` is absent.
    pub paths_base_path: Option<PathBuf>,
    pub root_dir: Option<PathBuf>,
    pub root_dirs: Vec<PathBuf>,
    pub out_dir: Option<PathBuf>,
    pub out_file: Option<PathBuf>,
    pub declaration_dir: Option<PathBuf>,
    pub composite: bool,
    pub emit_declarations: bool,
    pub emit_declaration_only: bool,
    pub source_map: bool,
    pub inline_source_map: bool,
    pub declaration_map: bool,
    pub ts_build_info_file: Option<PathBuf>,
    pub incremental: bool,
    pub no_emit: bool,
    pub emit_bom: bool,
    pub no_emit_on_error: bool,
    /// Skip module graph expansion from imports/references when checking.
    pub no_resolve: bool,
    /// Preserve symlink paths instead of canonicalizing to real paths.
    pub preserve_symlinks: bool,
    pub isolated_declarations: bool,
    pub import_helpers: bool,
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
    /// Whether `checkJs` was explicitly set to `false` in compiler options.
    /// When `true`, ALL semantic errors are suppressed in JS files — even the
    /// `plainJSErrors` allowlist (TS2451, TS2492, etc.) that applies in the
    /// default (no-`checkJs`) mode. Distinct from `check_js == false` because
    /// that default-false is the same as "not configured", which still permits
    /// `plainJSErrors`.
    pub explicit_check_js_false: bool,
    /// Skip type checking of declaration files (.d.ts)
    pub skip_lib_check: bool,
    /// Skip type checking of default library declaration files (.d.ts)
    pub skip_default_lib_check: bool,
    /// Disable emitting declarations that have '@internal' in their JSDoc comments
    pub strip_internal: bool,
    /// Maximum folder depth for checking JS files from `node_modules`.
    /// Only applicable with `allowJs`. Default: 0 (don't check JS in `node_modules`).
    pub max_node_module_js_depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsxEmit {
    Preserve,
    React,
    ReactJsx,
    ReactJsxDev,
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

impl ModuleResolutionKind {
    /// Parse a TypeScript compiler option `moduleResolution` value.
    ///
    /// This accepts tsc spelling variants.
    #[must_use]
    pub fn from_ts_str(value: &str) -> Option<Self> {
        let normalized = normalize_option(value.trim());
        match normalized.as_str() {
            "classic" => Some(Self::Classic),
            "node" | "node10" => Some(Self::Node),
            "node16" => Some(Self::Node16),
            "nodenext" => Some(Self::NodeNext),
            "bundler" => Some(Self::Bundler),
            _ => None,
        }
    }

    /// Return a canonical TypeScript option spelling.
    #[must_use]
    pub const fn as_ts_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Node => "node10",
            Self::Node16 => "node16",
            Self::NodeNext => "nodenext",
            Self::Bundler => "bundler",
        }
    }

    #[must_use]
    pub const fn is_modern(self) -> bool {
        matches!(self, Self::Node16 | Self::NodeNext | Self::Bundler)
    }
}

/// Default module kind used when `module` is omitted.
///
/// Faithful port of tsc 6.0's `_computedOptions.module.computeValue`
/// (`commandLineParser.ts`), which derives the module from the *effective*
/// target rather than the raw `target` option:
///
/// 1. `_computedOptions.target.computeValue` first folds an omitted target —
///    and an explicit `ES3`, which tsc 6.0 no longer treats as a distinct
///    emit target — into `LatestStandard` (`ES2025`).
/// 2. The module is then chosen from that effective target by descending
///    capability tiers: `ESNext -> ESNext`, `>= ES2022 -> ES2022`,
///    `>= ES2020 -> ES2020`, `>= ES2015 -> ES2015`, otherwise `CommonJS`.
///
/// This is what distinguishes tsc 6.0 from the older
/// `target >= ES2015 ? ES2015 : CommonJS` rule. The observable consequences,
/// both verified against the pinned tsc 6.0.2:
/// - with no `target`/`module`, the default module is `ES2022` (not `ESNext`),
///   so capability gates such as TS2823 (import attributes) fire by default
///   exactly as tsc does instead of being silently suppressed; and
/// - `--target es3` reports module `ES2022` (folded through `LatestStandard`)
///   rather than `CommonJS`, so `--showConfig` omits the implied `module` line
///   just as tsc does.
///
/// `target_explicitly_set` distinguishes an omitted `target` from one that was
/// supplied, because callers pass an already-defaulted [`ScriptTarget`]; only
/// the omitted case (and an explicit `ES3`) folds to `LatestStandard`.
///
/// The `LatestStandard` fold is intentionally *local* to module derivation and
/// must not be hoisted into a shared "effective target": lib and checker-target
/// resolution deliberately keep tsz's own default target (`ES2024`, set by
/// `PrinterOptions::default`) and preserve an explicit `ES3`, so sharing the
/// fold would change lib-file selection.
#[must_use]
pub const fn default_module_kind_for_target(
    target: ScriptTarget,
    target_explicitly_set: bool,
) -> ModuleKind {
    // Step 1: resolve the effective target. An omitted target and an explicit
    // `ES3` both collapse to `LatestStandard` (`ES2025`).
    let effective = if !target_explicitly_set || matches!(target, ScriptTarget::ES3) {
        ScriptTarget::ES2025
    } else {
        target
    };

    // Step 2: pick the module from the effective target by capability tier,
    // reusing the shared `ScriptTarget::supports_*` comparators. `ESNext` must
    // be matched before the tiers because it outranks every dated target.
    match effective {
        ScriptTarget::ESNext => ModuleKind::ESNext,
        t if t.supports_es2022() => ModuleKind::ES2022,
        t if t.supports_es2020() => ModuleKind::ES2020,
        t if t.supports_es2015() => ModuleKind::ES2015,
        _ => ModuleKind::CommonJS,
    }
}

/// Resolve the default `module` kind when it is not explicitly set, matching
/// tsc: `moduleResolution` `node16`/`nodenext` imply the matching module kind,
/// otherwise the module is derived from the effective target
/// (`default_module_kind_for_target`). This is the single source of truth shared
/// by both the tsconfig resolution and the CLI-override paths so a `--target` or
/// `--moduleResolution` override recomputes the same default.
#[must_use]
pub const fn derive_default_module_kind(
    target: ScriptTarget,
    target_explicitly_set: bool,
    module_resolution: Option<ModuleResolutionKind>,
) -> ModuleKind {
    match module_resolution {
        Some(ModuleResolutionKind::Node16) => ModuleKind::Node16,
        Some(ModuleResolutionKind::NodeNext) => ModuleKind::NodeNext,
        _ => default_module_kind_for_target(target, target_explicitly_set),
    }
}

/// Default `moduleResolution` used when it is omitted for a module kind.
#[must_use]
pub const fn default_module_resolution_for_module(module: ModuleKind) -> ModuleResolutionKind {
    match module {
        ModuleKind::None | ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System => {
            ModuleResolutionKind::Classic
        }
        ModuleKind::NodeNext => ModuleResolutionKind::NodeNext,
        ModuleKind::Node16 | ModuleKind::Node18 | ModuleKind::Node20 => {
            ModuleResolutionKind::Node16
        }
        ModuleKind::CommonJS
        | ModuleKind::ES2015
        | ModuleKind::ES2020
        | ModuleKind::ES2022
        | ModuleKind::ESNext
        | ModuleKind::Preserve => ModuleResolutionKind::Bundler,
    }
}

/// Record a resolved `moduleDetection` in every consumer's view of it.
///
/// `tsc` resolves the setting once (`getEmitModuleDetectionKind`) and hands the
/// same kind to the file-level module-ness predicate and to emit. tsz has two
/// representations — `checker.module_detection`, which the binder reads to
/// decide `is_external_module`, and the emitter's `module_detection_force` /
/// `module_detection_legacy` pair — so both are written here and can never
/// disagree.
pub fn apply_module_detection(resolved: &mut ResolvedCompilerOptions, kind: ModuleDetectionKind) {
    resolved.checker.module_detection = kind;
    resolved.printer.module_detection_force = kind == ModuleDetectionKind::Force;
    resolved.printer.module_detection_legacy = kind == ModuleDetectionKind::Legacy;
}

/// Default `moduleDetection` shown by tsc-style config output for a module kind.
#[must_use]
pub const fn default_module_detection_for_module(module: ModuleKind) -> &'static str {
    match module {
        ModuleKind::Node16 | ModuleKind::Node18 | ModuleKind::Node20 | ModuleKind::NodeNext => {
            "force"
        }
        _ => "auto",
    }
}

#[derive(Debug, Clone)]
pub struct PathMapping {
    pub pattern: String,
    pub(crate) prefix: String,
    pub(crate) suffix: String,
    pub targets: Vec<String>,
}

impl PathMapping {
    pub fn match_specifier(&self, specifier: &str) -> Option<String> {
        if !self.pattern.contains('*') {
            return (self.pattern == specifier).then(String::new);
        }

        // Keys with more than one `*` are rejected up front by
        // `build_path_mappings` (mirroring tsc's `tryParsePattern`), so every
        // mapping that reaches here has exactly one `*` and a well-formed
        // `prefix`/`suffix`. The capture itself shares the single-`*` matcher
        // with the `exports`/`imports` resolvers (this carries precomputed
        // `prefix`/`suffix` fields rather than re-splitting the pattern).
        match_prefix_suffix(&self.prefix, &self.suffix, specifier)
    }

    pub const fn specificity(&self) -> usize {
        self.prefix.len()
    }

    /// Substitute the wildcard text captured by [`match_specifier`](Self::match_specifier)
    /// into one of this mapping's targets, mirroring tsc's
    /// `tryLoadModuleUsingPaths`:
    ///
    /// - For a wildcard key, tsc computes `subst.replace("*", matchedStar)`.
    ///   JavaScript's `String.prototype.replace` with a string pattern replaces
    ///   only the **first** `*`, so a target like `"./gen/*/*.js"` becomes
    ///   `"./gen/<star>/*.js"` — the trailing `*` is left untouched. A target
    ///   with no `*` is used verbatim.
    /// - For an exact, wildcard-free key, tsc leaves `matchedStar` undefined and
    ///   uses the target verbatim (`path = subst`), so a literal `*` in the
    ///   target is preserved rather than stripped.
    ///
    /// Both the `tsz-core` resolver and the CLI driver route tsconfig-`paths`
    /// target substitution through this one method so the substituted path — and
    /// thus the file identity it resolves to — cannot drift between them. (Node
    /// package `exports`/`imports` substitution is a separate Node-spec concern
    /// and does not use this method.)
    pub fn substitute_target(&self, target: &str, captured: &str) -> String {
        if self.pattern.contains('*') {
            target.replacen('*', captured, 1)
        } else {
            target.to_string()
        }
    }

    /// Select the single tsc-best mapping in `mappings` for `specifier`,
    /// mirroring tsc's `matchPatternOrExact` -> `findBestPatternMatch`.
    ///
    /// An exact, wildcard-free key equal to the specifier wins outright;
    /// otherwise the matching wildcard with the longest prefix (highest
    /// [`specificity`](Self::specificity)) is chosen, keeping the first such
    /// mapping on a tie. The selection is computed explicitly, so it is correct
    /// for any ordering — it does not rely on `mappings` being pre-sorted by
    /// `build_path_mappings`. Returns the winning mapping's index together with
    /// the captured wildcard text (the substring the `*` matched, or an empty
    /// string for an exact key), so callers can both reference the mapping and
    /// cache the result by index.
    ///
    /// Both module resolvers (the `tsz-core` checker resolver and the CLI
    /// driver) route pattern selection through this one helper so the
    /// "exactly one pattern, no fall-through" rule has a single owner.
    pub fn select_best(mappings: &[PathMapping], specifier: &str) -> Option<(usize, String)> {
        let mut best: Option<(usize, String)> = None;
        let mut best_specificity = 0;
        for (idx, mapping) in mappings.iter().enumerate() {
            let Some(star_match) = mapping.match_specifier(specifier) else {
                continue;
            };
            if !mapping.pattern.contains('*') {
                // Exact wildcard-free key equal to the specifier wins outright.
                return Some((idx, star_match));
            }
            if best.is_none() || mapping.specificity() > best_specificity {
                best_specificity = mapping.specificity();
                best = Some((idx, star_match));
            }
        }
        best
    }
}

impl ResolvedCompilerOptions {
    pub const fn effective_module_resolution(&self) -> ModuleResolutionKind {
        if let Some(resolution) = self.module_resolution {
            return resolution;
        }

        default_module_resolution_for_module(self.printer.module)
    }

    /// Base directory for `paths` substitutions — tsc's `getPathsBasePath`,
    /// which returns `baseUrl ?? pathsBasePath`. Since TypeScript 4.1 `paths`
    /// may be configured without `baseUrl`, in which case relative
    /// substitutions resolve against the tsconfig directory carried in
    /// [`Self::paths_base_path`]. The bare `baseUrl` join fallback is NOT
    /// derived from this — it stays anchored on `base_url` alone.
    pub fn paths_base(&self) -> Option<&Path> {
        self.base_url.as_deref().or(self.paths_base_path.as_deref())
    }
}

/// Trim a configured string-list entry, dropping it when nothing remains.
///
/// Shared by the `types` / `typeRoots` / `rootDirs` normalizers, which differ
/// only in the final `String` vs `PathBuf` constructor applied to the trimmed
/// slice (`trimmed_non_empty(v).map(String::from)` /
/// `trimmed_non_empty(v).map(PathBuf::from)`).
fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

pub fn resolve_compiler_options(
    options: Option<&CompilerOptions>,
) -> Result<ResolvedCompilerOptions> {
    let mut resolved = ResolvedCompilerOptions::default();
    // TypeScript 6 defaults alwaysStrict emit on. An explicit
    // alwaysStrict=false below can still suppress the prologue.
    resolved.printer.always_strict = true;
    let Some(options) = options else {
        let default_module = default_module_kind_for_target(resolved.printer.target, false);
        resolved.printer.module = default_module;
        resolved.checker.module = default_module;
        resolved.checker.target = checker_target_from_emitter(resolved.printer.target);
        resolved.lib_files = resolve_default_lib_files(resolved.printer.target)?;
        resolved.lib_is_default = true;
        resolved.module_suffixes = vec![String::new()];
        let default_resolution = resolved.effective_module_resolution();
        resolved.resolve_package_json_exports = matches!(
            default_resolution,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        );
        resolved.resolve_package_json_imports = resolved.resolve_package_json_exports;
        let resolve_json_module = matches!(default_resolution, ModuleResolutionKind::Bundler);
        resolved.resolve_json_module = resolve_json_module;
        resolved.checker.resolve_json_module = resolve_json_module;
        return Ok(resolved);
    };

    if let Some(target) = options.target.as_deref() {
        resolved.printer.target = parse_script_target(target)?;
    }
    resolved.checker.target = checker_target_from_emitter(resolved.printer.target);

    if let Some(module_resolution) = options.module_resolution.as_deref() {
        let value = module_resolution.trim();
        if !value.is_empty() {
            resolved.module_resolution = Some(parse_module_resolution(value)?);
        }
    }

    let module_explicitly_set = options.module.is_some();
    if let Some(module) = options.module.as_deref() {
        let kind = parse_module_kind(module)?;
        resolved.printer.module = kind;
        resolved.checker.module = kind;
    } else {
        // No explicit module: derive it from moduleResolution (node16/nodenext)
        // or the effective target (matches tsc's `getEmitModuleKind`).
        let default_module = derive_default_module_kind(
            resolved.printer.target,
            options.target.is_some(),
            resolved.module_resolution,
        );
        resolved.printer.module = default_module;
        resolved.checker.module = default_module;
    }
    resolved.checker.module_explicitly_set = module_explicitly_set;
    let effective_resolution = resolved.effective_module_resolution();
    // TS2792 remains tied to Classic resolution sites in conformance. Keep the
    // downstream checker/resolver flag derived from the computed effective
    // module resolution instead of hard-disabling it globally.
    resolved.checker.implied_classic_resolution =
        matches!(effective_resolution, ModuleResolutionKind::Classic);
    resolved.resolve_package_json_exports = options.resolve_package_json_exports.unwrap_or({
        matches!(
            effective_resolution,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        )
    });
    // Per tsc 6.0, `resolvePackageJsonImports` defaults to true only for
    // Node16/NodeNext/Bundler. Legacy `node`/`node10` does NOT resolve
    // `package.json#imports` unless the option is explicitly enabled.
    resolved.resolve_package_json_imports = options.resolve_package_json_imports.unwrap_or({
        matches!(
            effective_resolution,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        )
    });
    if let Some(module_suffixes) = options.module_suffixes.as_ref() {
        resolved.module_suffixes = module_suffixes.clone();
    } else {
        resolved.module_suffixes = vec![String::new()];
    }
    if let Some(resolve_json_module) = options.resolve_json_module {
        resolved.resolve_json_module = resolve_json_module;
        resolved.checker.resolve_json_module = resolve_json_module;
    } else {
        // tsc 6.0 only implies resolveJsonModule for bundler resolution.
        let resolve_json_module = matches!(effective_resolution, ModuleResolutionKind::Bundler);
        resolved.resolve_json_module = resolve_json_module;
        resolved.checker.resolve_json_module = resolve_json_module;
    }
    if let Some(import_helpers) = options.import_helpers {
        resolved.import_helpers = import_helpers;
        resolved.printer.import_helpers = import_helpers;
    }
    if let Some(allow_arbitrary_extensions) = options.allow_arbitrary_extensions {
        resolved.allow_arbitrary_extensions = allow_arbitrary_extensions;
    }
    if let Some(allow_importing_ts_extensions) = options.allow_importing_ts_extensions {
        resolved.allow_importing_ts_extensions = allow_importing_ts_extensions;
    }
    if let Some(rewrite_relative_import_extensions) = options.rewrite_relative_import_extensions {
        resolved.rewrite_relative_import_extensions = rewrite_relative_import_extensions;
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
            .filter_map(|value| trimmed_non_empty(value).map(String::from))
            .collect();
        resolved.checker.types_has_wildcard = list.iter().any(|entry| entry == "*");
        resolved.types = Some(list);
        resolved.checker.types_explicitly_set = true;
    }

    if let Some(type_roots) = options.type_roots.as_ref() {
        let roots: Vec<PathBuf> = type_roots
            .iter()
            .filter_map(|value| trimmed_non_empty(value).map(PathBuf::from))
            .collect();
        resolved.type_roots = Some(roots);
    }

    if let Some(factory) = options.jsx_factory.as_deref() {
        // tsc preserves `jsxFactory` verbatim — even when invalid. The
        // TS5067 / TS5059 diagnostics surface separately during config
        // validation; emit uses whatever was configured.
        resolved.checker.jsx_factory = factory.to_string();
        resolved.checker.jsx_factory_from_config = true;
    } else if let Some(ns) = options.react_namespace.as_deref() {
        resolved.checker.jsx_factory = format!("{ns}.createElement");
    }
    if let Some(frag) = options.jsx_fragment_factory.as_deref() {
        // tsc falls back to `React.Fragment` when `jsxFragmentFactory` is not
        // a valid identifier chain (e.g. `234`). Asymmetric with `jsxFactory`
        // by design — see the test pair `reactNamespaceInvalidInput` (factory
        // preserved) vs `jsxFactoryAndJsxFragmentFactoryErrorNotIdentifier`
        // (fragment factory falls back).
        if is_valid_identifier_or_qualified_name(frag) {
            resolved.checker.jsx_fragment_factory = frag.to_string();
            resolved.checker.jsx_fragment_factory_from_config = true;
        }
        // else: keep default `React.Fragment`
    }
    if let Some(source) = options.jsx_import_source.as_deref() {
        resolved.checker.jsx_import_source = source.to_string();
    }

    if let Some(jsx) = options.jsx.as_deref() {
        let jsx_emit = parse_jsx_emit(jsx)?;
        resolved.jsx = Some(jsx_emit);
        resolved.checker.jsx_mode = jsx_emit_to_mode(jsx_emit);
    }

    if let Some(no_lib) = options.no_lib {
        resolved.checker.no_lib = no_lib;
    }

    if let Some(lib_replacement) = options.lib_replacement {
        resolved.lib_replacement = lib_replacement;
    }

    if resolved.checker.no_lib && options.lib.is_some() {
        return Err(anyhow::anyhow!(
            "Option 'lib' cannot be specified with option 'noLib'."
        ));
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
    } else if !resolved.checker.no_lib {
        // noTypesAndSymbols is a test harness directive that controls baseline
        // output (type/symbol baselines), NOT lib loading. Default libs must
        // still be loaded so that globals like Symbol, Promise, etc. are available.
        resolved.lib_files = resolve_default_lib_files(resolved.printer.target)?;
        resolved.lib_is_default = true;
    }

    let base_url = options.base_url.as_deref().map(str::trim);
    if let Some(base_url) = base_url
        && !base_url.is_empty()
    {
        resolved.base_url = Some(PathBuf::from(base_url));
    }

    if let Some(paths) = options.paths.as_ref()
        && !paths.is_empty()
    {
        resolved.paths = Some(build_path_mappings(paths));
    }

    if let Some(root_dir) = options.root_dir.as_deref()
        && !root_dir.is_empty()
    {
        resolved.root_dir = Some(PathBuf::from(root_dir));
    }

    if let Some(root_dirs) = options.root_dirs.as_ref() {
        resolved.root_dirs = root_dirs
            .iter()
            .filter_map(|value| trimmed_non_empty(value).map(PathBuf::from))
            .collect();
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

    // composite implies declaration and incremental. The implication itself is
    // owned by the shared `apply_non_strict_fanout` table (tsc 6.0.3
    // `computedOptions.declaration`/`incremental`); here we only record the
    // raw `composite` value so the table can fan it out below.
    if let Some(composite) = options.composite {
        resolved.composite = composite;
    }

    if let Some(declaration) = options.declaration {
        resolved.emit_declarations = declaration;
        resolved.checker.emit_declarations = declaration;
    }

    if let Some(emit_declaration_only) = options.emit_declaration_only {
        resolved.emit_declaration_only = emit_declaration_only;
    }

    if let Some(source_map) = options.source_map {
        resolved.source_map = source_map;
    }

    if let Some(inline_source_map) = options.inline_source_map {
        resolved.inline_source_map = inline_source_map;
    }

    if let Some(declaration_map) = options.declaration_map {
        resolved.declaration_map = declaration_map;
    }

    if let Some(no_emit_helpers) = options.no_emit_helpers {
        resolved.printer.no_emit_helpers = no_emit_helpers;
    }
    // `importHelpers` suppressing inline helper emission is owned by the shared
    // `apply_non_strict_fanout` table; `resolved.import_helpers` is recorded
    // above so the table can fan it out below.

    if let Some(downlevel_iteration) = options.downlevel_iteration {
        resolved.printer.downlevel_iteration = downlevel_iteration;
        resolved.checker.downlevel_iteration = downlevel_iteration;
    }

    if let Some(remove_comments) = options.remove_comments {
        resolved.printer.remove_comments = remove_comments;
    }

    if let Some(new_line) = options.new_line.as_deref() {
        resolved.printer.new_line = parse_new_line_kind(new_line)?;
    }

    if let Some(ts_build_info_file) = options.ts_build_info_file.as_deref()
        && !ts_build_info_file.is_empty()
    {
        resolved.ts_build_info_file = Some(PathBuf::from(ts_build_info_file));
    }

    if let Some(incremental) = options.incremental {
        resolved.incremental = incremental;
    }

    // Strict-family expansion and explicit member overrides are owned by the
    // shared `strict_family` table (tsc 6.0 `getStrictOptionValue`): the
    // `strict` umbrella is expanded first, then explicitly provided members
    // win (issue #3861 ordering). `alwaysStrict` is not a family member in
    // tsc 6.0 (`alwaysStrict !== false`, independent of `strict`); only the
    // explicit `alwaysStrict` override below touches it.
    apply_strict_family(
        &mut resolved.checker,
        &StrictFamilyOverrides {
            strict: options.strict,
            no_implicit_any: options.no_implicit_any,
            no_implicit_this: options.no_implicit_this,
            strict_null_checks: options.strict_null_checks,
            strict_function_types: options.strict_function_types,
            strict_bind_call_apply: options.strict_bind_call_apply,
            strict_property_initialization: options.strict_property_initialization,
            strict_builtin_iterator_return: options.strict_builtin_iterator_return,
            use_unknown_in_catch_variables: options.use_unknown_in_catch_variables,
        },
    );
    if options.strict_builtin_iterator_return.is_none()
        && options
            .invalidated_options
            .iter()
            .any(|key| key == "strictBuiltinIteratorReturn")
        && let Some(strict) = options.strict
    {
        // tsc reports TS5024 for an invalid explicitly-provided
        // strictBuiltinIteratorReturn value, but the invalid sub-option does
        // not block the strict umbrella from selecting the effective value.
        resolved.checker.strict_builtin_iterator_return = strict;
    }

    if let Some(sound) = options.sound {
        resolved.checker.sound_mode = sound;
    }
    if let Some(v) = options.sound_check_declarations {
        resolved.checker.sound_check_declarations = v;
    }
    if let Some(v) = options.sound_report_only {
        resolved.checker.sound_report_only = v;
    }
    if let Some(v) = options.sound_pedantic {
        resolved.checker.sound_pedantic = v;
    }

    // tsc 6.0 defaults: strict-family options are true when not explicitly set.
    // The tsc cache was generated with tsc 6.0-dev which has strict=true as its
    // effective default. CheckerOptions::default() already reflects this
    // (strict=true, all sub-flags=true). No override needed here.

    // Non-strict-family individual options. The strict-family members are
    // resolved by `apply_strict_family` above.
    if let Some(v) = options.no_implicit_returns {
        resolved.checker.no_implicit_returns = v;
    }
    if let Some(v) = options.no_unchecked_indexed_access {
        resolved.checker.no_unchecked_indexed_access = v;
    }
    if let Some(v) = options.exact_optional_property_types {
        resolved.checker.exact_optional_property_types = v;
    }
    if let Some(v) = options.no_property_access_from_index_signature {
        resolved.checker.no_property_access_from_index_signature = v;
    }
    if let Some(v) = options.no_implicit_override {
        resolved.checker.no_implicit_override = v;
    }
    if let Some(v) = options.no_unchecked_side_effect_imports {
        resolved.checker.no_unchecked_side_effect_imports = v;
    }

    if let Some(no_emit) = options.no_emit {
        resolved.no_emit = no_emit;
    }
    if let Some(emit_bom) = options.emit_bom {
        resolved.emit_bom = emit_bom;
    }
    if let Some(no_check) = options.no_check {
        resolved.no_check = no_check;
    }
    if let Some(no_resolve) = options.no_resolve {
        resolved.no_resolve = no_resolve;
        resolved.checker.no_resolve = no_resolve;
    }
    if let Some(preserve_symlinks) = options.preserve_symlinks {
        resolved.preserve_symlinks = preserve_symlinks;
    }

    if let Some(no_emit_on_error) = options.no_emit_on_error {
        resolved.no_emit_on_error = no_emit_on_error;
    }

    if let Some(isolated_modules) = options.isolated_modules {
        resolved.checker.isolated_modules = isolated_modules;
    }

    // Record the raw `verbatimModuleSyntax` value; the
    // `verbatimModuleSyntax -> isolatedModules` implication and the const-enum
    // printer mirroring are owned by the shared `apply_non_strict_fanout`
    // table (tsc 6.0.3 `computedOptions.isolatedModules`/`preserveConstEnums`).
    if options.verbatim_module_syntax == Some(true) {
        resolved.checker.verbatim_module_syntax = true;
    }

    if let Some(always_strict) = options.always_strict {
        resolved.checker.always_strict = always_strict;
        resolved.printer.always_strict = always_strict;
    }

    if let Some(use_define_for_class_fields) = options.use_define_for_class_fields {
        resolved.printer.use_define_for_class_fields = use_define_for_class_fields;
    }
    resolved.checker.use_define_for_class_fields = options.use_define_for_class_fields;

    if let Some(no_unused_locals) = options.no_unused_locals {
        resolved.checker.no_unused_locals = no_unused_locals;
    }

    if let Some(no_unused_parameters) = options.no_unused_parameters {
        resolved.checker.no_unused_parameters = no_unused_parameters;
    }

    if let Some(allow_unreachable_code) = options.allow_unreachable_code {
        resolved.checker.allow_unreachable_code = Some(allow_unreachable_code);
    }

    if let Some(allow_unused_labels) = options.allow_unused_labels {
        resolved.checker.allow_unused_labels = Some(allow_unused_labels);
    }

    if let Some(ref id) = options.ignore_deprecations
        && (id == "5.0" || id == "6.0" || id == "7.0")
    {
        resolved.checker.ignore_deprecations = true;
        // No accepted value silences TS2880 (see #16217); this only carries
        // the version distinction for future version-gated deprecations.
        // Mirrors the CLI override path in `tsz-cli`'s `driver::plan`.
        resolved.checker.ignore_deprecations_6_0 = id == "6.0";
    }

    if let Some(allow_umd) = options.allow_umd_global_access {
        resolved.checker.allow_umd_global_access = allow_umd;
    }

    if let Some(preserve) = options.preserve_const_enums {
        resolved.checker.preserve_const_enums = preserve;
        resolved.printer.preserve_const_enums = preserve;
    }

    if let Some(erasable) = options.erasable_syntax_only {
        resolved.checker.erasable_syntax_only = erasable;
    }

    if let Some(no_fallthrough) = options.no_fallthrough_cases_in_switch {
        resolved.checker.no_fallthrough_cases_in_switch = no_fallthrough;
    }

    if let Some(ref custom_conditions) = options.custom_conditions {
        resolved.custom_conditions = custom_conditions.clone();
    }

    let esmodule_invalidated = options
        .invalidated_options
        .iter()
        .any(|k| k == "esModuleInterop");
    // The `esModuleInterop -> allowSyntheticDefaultImports` implication is owned
    // by the shared `apply_non_strict_fanout` table below; this block only
    // resolves the `esModuleInterop` value/default (engine-local because it
    // depends on TS5024 invalidation state).
    if let Some(es_module_interop) = options.es_module_interop
        && !esmodule_invalidated
    {
        resolved.es_module_interop = es_module_interop;
        resolved.checker.es_module_interop = es_module_interop;
        resolved.printer.es_module_interop = es_module_interop;
    } else if !esmodule_invalidated {
        // tsc 6.0 defaults esModuleInterop to true when not explicitly set.
        // But do NOT apply the default when TS5024 fired for this option —
        // tsc treats a type-mismatched value as if the option was never set
        // (no default, stays false).
        resolved.es_module_interop = true;
        resolved.checker.es_module_interop = true;
        resolved.printer.es_module_interop = true;
    }

    // Fan out the non-strict-family implications (composite -> declaration +
    // incremental, isolatedModules/verbatimModuleSyntax -> preserveConstEnums,
    // importHelpers -> no_emit_helpers, esModuleInterop ->
    // allowSyntheticDefaultImports) through the shared table. Runs before the
    // explicit `allowSyntheticDefaultImports` override below so an explicit
    // value still wins over the `esModuleInterop` implication.
    super::apply_non_strict_fanout(&mut resolved);

    if let Some(allow_synthetic_default_imports) = options.allow_synthetic_default_imports {
        resolved.allow_synthetic_default_imports = allow_synthetic_default_imports;
        resolved.checker.allow_synthetic_default_imports = allow_synthetic_default_imports;
    } else if !resolved.allow_synthetic_default_imports {
        // TSC defaults allowSyntheticDefaultImports to true when:
        // - esModuleInterop is true (already handled above)
        // - module is "system"
        // - moduleResolution is "bundler"
        // Otherwise defaults to false.
        let should_default_true = matches!(resolved.checker.module, ModuleKind::System)
            || matches!(
                resolved.module_resolution,
                Some(ModuleResolutionKind::Bundler)
            );
        if should_default_true {
            resolved.allow_synthetic_default_imports = true;
            resolved.checker.allow_synthetic_default_imports = true;
        }
    }

    if let Some(experimental_decorators) = options.experimental_decorators {
        resolved.checker.experimental_decorators = experimental_decorators;
        resolved.printer.legacy_decorators = experimental_decorators;
    }

    if let Some(emit_decorator_metadata) = options.emit_decorator_metadata {
        resolved.printer.emit_decorator_metadata = emit_decorator_metadata;
    }

    if let Some(allow_js) = options.allow_js {
        resolved.allow_js = allow_js;
        resolved.checker.allow_js = allow_js;
    }

    if let Some(max_depth) = options.max_node_module_js_depth {
        resolved.max_node_module_js_depth = max_depth;
    }

    if let Some(check_js) = options.check_js {
        resolved.check_js = check_js;
        resolved.checker.check_js = check_js;
        if check_js && options.allow_js.is_none() {
            resolved.allow_js = true;
            resolved.checker.allow_js = true;
        }
        if !check_js {
            // Record that `checkJs: false` was explicit, not just the default.
            // This suppresses even the `plainJSErrors` allowlist (TS2451, etc.).
            resolved.explicit_check_js_false = true;
        }
    }
    if let Some(skip_lib_check) = options.skip_lib_check {
        resolved.skip_lib_check = skip_lib_check;
    }
    if let Some(skip_default_lib_check) = options.skip_default_lib_check {
        resolved.skip_default_lib_check = skip_default_lib_check;
    }
    if let Some(isolated_declarations) = options.isolated_declarations {
        resolved.isolated_declarations = isolated_declarations;
        resolved.checker.isolated_declarations = isolated_declarations;
    }
    if let Some(strip_internal) = options.strip_internal {
        resolved.strip_internal = strip_internal;
    }

    // Implement tsc's getEmitModuleDetectionKind:
    // - If moduleDetection is explicitly "force", all non-declaration files are modules.
    // - If moduleDetection is explicitly "auto" or "legacy", use their respective rules.
    // - If moduleDetection is NOT set and module is Node16-NodeNext, default to "force".
    // - If moduleDetection is NOT set and module is anything else, default to "auto".
    if let Some(ref module_detection) = options.module_detection {
        // An unrecognized value keeps tsc's own fallback of `auto`.
        let kind = ModuleDetectionKind::from_option_str(module_detection).unwrap_or_default();
        apply_module_detection(&mut resolved, kind);
    } else if resolved.printer.module.is_node_module() {
        // tsc defaults to Force for Node16/Node18/Node20/NodeNext
        apply_module_detection(&mut resolved, ModuleDetectionKind::Force);
    }

    Ok(resolved)
}

#[cfg(test)]
mod path_mapping_selection_tests {
    use super::PathMapping;

    fn mapping(pattern: &str, targets: &[&str]) -> PathMapping {
        let (prefix, suffix) = match pattern.find('*') {
            Some(star) => (pattern[..star].to_string(), pattern[star + 1..].to_string()),
            None => (pattern.to_string(), String::new()),
        };
        PathMapping {
            pattern: pattern.to_string(),
            prefix,
            suffix,
            targets: targets.iter().map(|t| t.to_string()).collect(),
        }
    }

    #[test]
    fn exact_key_beats_equal_prefix_wildcard_regardless_of_order() {
        // `matchPatternOrExact` returns an exact wildcard-free key before any
        // wildcard. `"alias"` and `"alias*"` tie on prefix length for the
        // specifier `"alias"`; the literal key must win in either ordering.
        for order in [
            vec![
                mapping("alias", &["./exact"]),
                mapping("alias*", &["./wild"]),
            ],
            vec![
                mapping("alias*", &["./wild"]),
                mapping("alias", &["./exact"]),
            ],
        ] {
            let (idx, star) =
                PathMapping::select_best(&order, "alias").expect("a mapping must be selected");
            assert_eq!(order[idx].pattern, "alias", "exact key must win the tie");
            assert_eq!(star, "");
        }
    }

    #[test]
    fn longest_prefix_wildcard_wins_independent_of_order() {
        // The longest-prefix wildcard is chosen even when the input is not
        // pre-sorted by specificity, proving the selection does not depend on
        // `build_path_mappings`' ordering.
        let unsorted = vec![
            mapping("*", &["./external.d.ts"]),
            mapping("next/dist/*", &["./src/*"]),
            mapping("next/dist/compiled/*", &["./compiled/*"]),
        ];
        let (idx, star) = PathMapping::select_best(&unsorted, "next/dist/compiled/react")
            .expect("a wildcard must be selected");
        assert_eq!(unsorted[idx].pattern, "next/dist/compiled/*");
        assert_eq!(star, "react");
    }

    #[test]
    fn no_match_returns_none() {
        let mappings = vec![mapping("@app/*", &["./src/*"])];
        assert!(PathMapping::select_best(&mappings, "unrelated/thing").is_none());
    }

    #[test]
    fn multi_star_key_is_dropped_at_build_like_tsc_try_parse_pattern() {
        use super::build_path_mappings;
        use rustc_hash::FxHashMap;

        // tsc's `tryParsePattern` returns `undefined` for a key with two `*`, so
        // `tryParsePatterns` never builds a mapping for it. `build_path_mappings`
        // must drop it at the parser so it can never match a specifier (which it
        // would, on its mis-derived first-`*` `prefix`/`suffix`).
        let mut paths: FxHashMap<String, Vec<String>> = FxHashMap::default();
        paths.insert("a/*/*".to_string(), vec!["./wrong/*".to_string()]);
        paths.insert("*".to_string(), vec!["./types/*".to_string()]);

        let mappings = build_path_mappings(&paths);
        assert!(
            mappings.iter().all(|m| m.pattern != "a/*/*"),
            "multi-`*` key must be dropped at build time"
        );

        // With the malformed key gone, only the valid catch-all can match.
        let (idx, star) =
            PathMapping::select_best(&mappings, "a/foo/bar").expect("catch-all must match");
        assert_eq!(mappings[idx].pattern, "*");
        assert_eq!(star, "a/foo/bar");
    }

    #[test]
    fn single_star_key_still_matches() {
        // The multi-`*` guard must not regress an ordinary single-`*` wildcard.
        let one_star = mapping("@app/*", &["./src/*"]);
        assert_eq!(
            one_star.match_specifier("@app/util"),
            Some("util".to_string())
        );
    }

    #[test]
    fn wildcard_target_substitutes_only_first_star() {
        // tsc uses `subst.replace("*", matchedStar)`, which replaces only the
        // first `*`. A target with a second `*` keeps it verbatim.
        let m = mapping("@gen/*", &["./gen/*/*.js"]);
        let star = m.match_specifier("@gen/foo").expect("wildcard matches");
        assert_eq!(m.substitute_target(&m.targets[0], &star), "./gen/foo/*.js");
    }

    #[test]
    fn wildcard_target_without_star_is_used_verbatim() {
        let m = mapping("@fallback/*", &["./shim.d.ts"]);
        let star = m
            .match_specifier("@fallback/anything")
            .expect("wildcard matches");
        assert_eq!(m.substitute_target(&m.targets[0], &star), "./shim.d.ts");
    }

    #[test]
    fn wildcard_target_with_empty_capture_substitutes_empty() {
        // Specifier equal to prefix+suffix captures an empty `*`; tsc still
        // substitutes (the empty string), unlike an exact key.
        let m = mapping("@app/*", &["./src/*"]);
        let star = m.match_specifier("@app/").expect("empty capture matches");
        assert_eq!(star, "");
        assert_eq!(m.substitute_target(&m.targets[0], &star), "./src/");
    }

    #[test]
    fn exact_key_target_is_used_verbatim_keeping_literal_star() {
        // For an exact, wildcard-free key tsc leaves `matchedStar` undefined and
        // uses the target verbatim (`path = subst`), so a literal `*` in the
        // target is preserved rather than stripped.
        let m = mapping("foo", &["./bar/*"]);
        let star = m.match_specifier("foo").expect("exact key matches");
        assert_eq!(star, "");
        assert_eq!(m.substitute_target(&m.targets[0], &star), "./bar/*");
    }
}
