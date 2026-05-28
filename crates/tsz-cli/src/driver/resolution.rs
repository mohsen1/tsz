use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

use crate::config::{ModuleResolutionKind, PathMapping, ResolvedCompilerOptions};
use crate::fs::is_valid_module_or_js_file;
use tsz::module_resolver::{ImportKind, ImportingModuleKind, PackageType, is_path_relative};
use tsz::parser::NodeIndex;

type CollectedModuleSpecifier = (String, NodeIndex, ImportKind, Option<ImportingModuleKind>);

type SourceDiscoveryModuleRequest = (String, ImportKind, Option<ImportingModuleKind>, bool);

#[derive(Clone, Copy)]
enum AmbientModuleDeclarationSpecifierPolicy {
    #[cfg(test)]
    All,
    SourceDiscovery,
    Check {
        is_external_module: bool,
    },
}

// ─────────────────────────────────────────────────────────────────────────
// Counting filesystem probes (`PERFORMANCE_PLAN.md` §4.T0.3 follow-up)
//
// The plan calls for `resolver.is_file/is_dir/read_dir` counters so the
// 2026-05-10 attribution decision matrix can prove (or refute) that the
// resolver is on the hot path. Rather than sprinkle inline `inc()` calls
// before every `Path::is_file()`, we wrap the three probe primitives in
// thin counting helpers and route resolver code through them. Call sites
// in this file now use `count_is_file(p)` instead of `p.is_file()`, which
// keeps the diff one token per call and makes future swaps to a real
// `CountingFs` trait (the eventual T2.0 wrapper) a one-place change.
//
// The counter bumps themselves now live in `tsz_common::perf_counters`
// as `record_resolver_*` helpers (sibling to the cross-arena / interner
// helpers consolidated in #5097 / #5103 / #5112 / #5115 / #5118). Each
// wrapper here keeps its file-local identity because it bundles the
// counter with the underlying syscall — that's intentional grouping —
// while the gate-and-deref pattern lives in one place.
#[inline]
fn count_is_file(path: &Path) -> bool {
    tsz_common::perf_counters::record_resolver_is_file();
    path.is_file()
}

#[inline]
fn count_is_dir(path: &Path) -> bool {
    tsz_common::perf_counters::record_resolver_is_dir();
    path.is_dir()
}

#[inline]
fn count_read_dir(path: &Path) -> std::io::Result<std::fs::ReadDir> {
    tsz_common::perf_counters::record_resolver_read_dir();
    std::fs::read_dir(path)
}

/// Bump `resolver_candidate_paths_total` once per invocation. The
/// `tsz_common::perf_counters::record_resolver_candidate_path` helper
/// gates and dereferences once; this wrapper preserves the file-local
/// `count_candidate_path` name so the two emit sites
/// (path-mapping virtual roots and suffix-extension expansion) stay
/// stable.
#[inline]
fn count_candidate_path() {
    tsz_common::perf_counters::record_resolver_candidate_path();
}

/// Bump `resolver_read_package_json_calls` once per uncached read.
/// Sits inside `read_package_json_uncached`, which `large-ts-repo`
/// profiles flag as the dominant resolver work — keeping the gate
/// cheap matters even though the surrounding `read_to_string` is
/// several orders of magnitude more expensive.
#[inline]
fn count_read_package_json() {
    tsz_common::perf_counters::record_resolver_read_package_json();
}

#[derive(Default)]
pub(crate) struct ModuleResolutionCache {
    package_type_by_dir: FxHashMap<PathBuf, Option<PackageType>>,
    package_json_by_path: FxHashMap<PathBuf, Option<PackageJson>>,
    file_exists_by_path: FxHashMap<PathBuf, bool>,
    node_modules_dir_by_path: FxHashMap<PathBuf, bool>,
    package_root_dir_by_path: FxHashMap<PathBuf, bool>,
    // Per-compiler-options cache. A compile uses one resolved `paths` table, so
    // the specifier alone is enough to memoize the best matching mapping.
    path_mapping_by_specifier: FxHashMap<String, Option<(usize, String)>>,
}

fn package_relative_target_path(package_root: &Path, target: &str) -> Option<PathBuf> {
    let rest = target.strip_prefix("./")?;
    let path = Path::new(target);
    if path.components().any(|component| match component {
        Component::ParentDir | Component::RootDir | Component::Prefix(_) => true,
        Component::Normal(segment) => segment == "node_modules",
        _ => false,
    }) {
        return None;
    }
    Some(package_root.join(rest))
}

impl ModuleResolutionCache {
    fn file_exists(&mut self, path: &Path) -> bool {
        if let Some(&exists) = self.file_exists_by_path.get(path) {
            return exists;
        }

        let exists = count_is_file(path);
        self.file_exists_by_path.insert(path.to_path_buf(), exists);
        exists
    }

    fn read_package_json(&mut self, path: &Path) -> Option<PackageJson> {
        if let Some(cached) = self.package_json_by_path.get(path) {
            return cached.clone();
        }

        let parsed = read_package_json_uncached(path);
        self.package_json_by_path
            .insert(path.to_path_buf(), parsed.clone());
        parsed
    }

    fn node_modules_dir_exists(&mut self, path: &Path) -> bool {
        if let Some(&exists) = self.node_modules_dir_by_path.get(path) {
            return exists;
        }

        let exists = count_is_dir(path);
        self.node_modules_dir_by_path
            .insert(path.to_path_buf(), exists);
        exists
    }

    pub(crate) fn package_root_dir_exists(&mut self, path: &Path) -> bool {
        if let Some(&exists) = self.package_root_dir_by_path.get(path) {
            return exists;
        }

        let exists = count_is_dir(path);
        self.package_root_dir_by_path
            .insert(path.to_path_buf(), exists);
        exists
    }

    fn select_path_mapping<'a>(
        &mut self,
        mappings: &'a [PathMapping],
        specifier: &str,
    ) -> Option<(&'a PathMapping, String)> {
        if let Some(cached) = self.path_mapping_by_specifier.get(specifier) {
            return cached.as_ref().and_then(|(idx, wildcard)| {
                mappings
                    .get(*idx)
                    .map(|mapping| (mapping, wildcard.clone()))
            });
        }

        let selected = select_path_mapping(mappings, specifier);
        self.path_mapping_by_specifier
            .insert(specifier.to_string(), selected.clone());
        selected.and_then(|(idx, wildcard)| mappings.get(idx).map(|mapping| (mapping, wildcard)))
    }

    fn package_type_for_dir(&mut self, dir: &Path, base_dir: &Path) -> Option<PackageType> {
        let mut current = dir;
        let mut visited = Vec::new();

        loop {
            if let Some(value) = self.package_type_by_dir.get(current).copied() {
                for path in visited {
                    self.package_type_by_dir.insert(path, value);
                }
                return value;
            }

            visited.push(current.to_path_buf());

            if let Some(package_json) = self.read_package_json(&current.join("package.json")) {
                let value = package_type_from_json(Some(&package_json));
                for path in visited {
                    self.package_type_by_dir.insert(path, value);
                }
                return value;
            }

            if current == base_dir {
                for path in visited {
                    self.package_type_by_dir.insert(path, None);
                }
                return None;
            }

            let Some(parent) = current.parent() else {
                for path in visited {
                    self.package_type_by_dir.insert(path, None);
                }
                return None;
            };
            current = parent;
        }
    }
}

pub(crate) fn resolve_type_package_from_roots_with_cache(
    name: &str,
    roots: &[PathBuf],
    options: &ResolvedCompilerOptions,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    for root in roots {
        let candidates = type_package_candidates_for_root(name, root);
        if candidates.is_empty() {
            continue;
        }
        for candidate in &candidates {
            let package_root = root.join(candidate);
            if resolution_cache.package_root_dir_exists(&package_root)
                && let Some(entry) =
                    resolve_type_package_entry_with_cache(&package_root, options, resolution_cache)
            {
                return Some(entry);
            }

            if let Some(entry) = package_lookup::resolve_declaration_package_entry(
                root,
                candidate,
                options,
                None,
                resolution_cache,
            ) {
                return Some(entry);
            }
        }
    }

    None
}

/// Resolve a `/// <reference types="..." />` directive by searching `node_modules/`
/// directories walking up from the source file. This is the fallback used in
/// Node16/NodeNext/Bundler module resolution when type roots don't contain the package.
///
/// In tsc, `resolveTypeReferenceDirective` uses the regular module resolution algorithm
/// as a fallback after checking type roots. This means packages in `node_modules/`
/// (not just `node_modules/@types/`) can be found via triple-slash type references.
///
/// The resolution mode is determined by either:
/// - The explicit `resolution-mode` attribute (if present)
/// - The source file's module format (CJS → `require`, ESM → `import`)
pub(crate) fn resolve_type_reference_from_node_modules_with_cache(
    name: &str,
    from_file: &Path,
    base_dir: &Path,
    resolution_mode: Option<&str>,
    options: &ResolvedCompilerOptions,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    // Determine effective resolution mode from explicit attribute or file format
    let effective_mode = resolution_mode.map(String::from).unwrap_or_else(|| {
        implied_resolution_mode_for_file_with_cache(from_file, base_dir, resolution_cache)
    });

    // Generate all candidate package names (original + @types mangled form)
    let candidates = type_package_candidates(name);
    let package_subpath = package_lookup::split_package_specifier(name)
        .and_then(|(package_name, subpath)| subpath.map(|subpath| (package_name, subpath)));
    let conditions = package_lookup::export_conditions(options);

    let mut current = from_file.parent().unwrap_or(base_dir);

    loop {
        let node_modules = current.join("node_modules");
        if resolution_cache.node_modules_dir_exists(&node_modules) {
            if let Some((package_name, subpath)) = package_subpath.as_ref() {
                let package_root = node_modules.join(package_name);
                if resolution_cache.package_root_dir_exists(&package_root) {
                    let package_json =
                        resolution_cache.read_package_json(&package_root.join("package.json"));
                    let resolved = package_lookup::resolve_package_specifier(
                        &package_root,
                        Some(subpath),
                        package_json.as_ref(),
                        &conditions,
                        options,
                        resolution_cache,
                    );
                    if resolved.as_deref().is_some_and(is_declaration_file) {
                        return resolved;
                    }
                }
            }

            for candidate in &candidates {
                let package_root = node_modules.join(candidate);
                if resolution_cache.package_root_dir_exists(&package_root) {
                    let resolved = resolve_type_package_entry_with_mode_and_cache(
                        &package_root,
                        &effective_mode,
                        options,
                        resolution_cache,
                    );
                    if resolved.is_some() {
                        return resolved;
                    }
                    // Fall back to non-conditional resolution (types/typings/main/index)
                    let resolved = resolve_type_package_entry_with_cache(
                        &package_root,
                        options,
                        resolution_cache,
                    );
                    if resolved.is_some() {
                        return resolved;
                    }
                }
            }
        }

        if current == base_dir {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }

    None
}

/// Determine the implied resolution mode ("import" or "require") for a file
/// based on its extension and nearest `package.json` `type` field.
///
/// In Node16/NodeNext:
/// - `.mts`/`.mjs` files → ESM → "import"
/// - `.cts`/`.cjs` files → CJS → "require"
/// - `.ts`/`.tsx`/`.js`/`.jsx` files → depends on nearest `package.json`:
///   - `"type": "module"` → "import"
///   - otherwise → "require"
pub(super) fn implied_resolution_mode_for_file(file: &Path, base_dir: &Path) -> String {
    let mut cache = ModuleResolutionCache::default();
    implied_resolution_mode_for_file_with_cache(file, base_dir, &mut cache)
}

pub(super) fn implied_resolution_mode_for_file_with_cache(
    file: &Path,
    base_dir: &Path,
    resolution_cache: &mut ModuleResolutionCache,
) -> String {
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "mts" | "mjs" => return "import".to_string(),
        "cts" | "cjs" => return "require".to_string(),
        _ => {}
    }

    // Walk up from the file to find the nearest package.json with "type" field
    let mut current = file.parent().unwrap_or(base_dir);
    loop {
        let pkg_json_path = current.join("package.json");
        if let Some(pj) = resolution_cache.read_package_json(&pkg_json_path) {
            if pj.package_type.as_deref() == Some("module") {
                return "import".to_string();
            }
            // Found a package.json without "type": "module" → CJS
            return "require".to_string();
        }
        if current == base_dir {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }

    // Default to require (CJS) when no package.json is found
    "require".to_string()
}

/// Public wrapper for `type_package_candidates`.
pub(crate) fn type_package_candidates_pub(name: &str) -> Vec<String> {
    type_package_candidates(name)
}

fn type_package_candidates_for_root(name: &str, root: &Path) -> Vec<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let normalized = trimmed.replace('\\', "/");
    let mut candidates = Vec::new();
    let is_at_types_root = root.file_name().and_then(|name| name.to_str()) == Some("@types");

    if let Some(stripped) = normalized.strip_prefix("@types/")
        && !stripped.is_empty()
    {
        candidates.push(stripped.to_string());
    }

    if let Some(stripped) = normalized.strip_prefix('@')
        && !normalized.starts_with("@types/")
        && let Some((scope, pkg)) = stripped.split_once('/')
        && !scope.is_empty()
        && !pkg.is_empty()
    {
        let mangled = format!("{scope}__{pkg}");
        if is_at_types_root {
            candidates.push(mangled);
        } else {
            candidates.push(normalized.clone());
        }
        return candidates;
    }

    if !normalized.starts_with('@') && !normalized.contains('/') {
        let at_types = format!("@types/{normalized}");
        if !candidates.iter().any(|v| v == &at_types) {
            candidates.push(at_types);
        }
    }

    if !candidates.iter().any(|value| value == &normalized) {
        candidates.push(normalized);
    }

    candidates
}

fn type_package_candidates(name: &str) -> Vec<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let normalized = trimmed.replace('\\', "/");
    let mut candidates = Vec::new();

    if let Some(stripped) = normalized.strip_prefix("@types/")
        && !stripped.is_empty()
    {
        candidates.push(stripped.to_string());
    }

    // Scoped package mangling: @scope/name → @types/scope__name
    // tsc resolves `/// <reference types="@scope/name" />` by checking both
    // the original scoped path and the @types-mangled equivalent.
    if let Some(stripped) = normalized.strip_prefix('@')
        && !normalized.starts_with("@types/")
        && let Some((scope, pkg)) = stripped.split_once('/')
        && !scope.is_empty()
        && !pkg.is_empty()
    {
        let plain_mangled = format!("{scope}__{pkg}");
        candidates.push(plain_mangled);
        candidates.push(format!("@types/@{scope}/{pkg}"));
        let mangled = format!("@types/{scope}__{pkg}");
        candidates.push(mangled);
    }

    // For bare (non-scoped) package names, also check @types/<name>.
    // tsc's resolveTypeReferenceDirective checks both node_modules/<name>/
    // and node_modules/@types/<name>/ during the walk-up.
    if !normalized.starts_with('@') && !normalized.contains('/') {
        let at_types = format!("@types/{normalized}");
        if !candidates.iter().any(|v| v == &at_types) {
            candidates.push(at_types);
        }
    }

    if !candidates.iter().any(|value| value == &normalized) {
        candidates.push(normalized);
    }

    candidates
}

pub(crate) fn collect_type_packages_from_root(root: &Path) -> Vec<PathBuf> {
    let mut packages = Vec::new();
    let entries = match count_read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return packages,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !count_is_dir(&path) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if name.starts_with('@') {
            if let Ok(scope_entries) = count_read_dir(&path) {
                for scope_entry in scope_entries.flatten() {
                    let scope_path = scope_entry.path();
                    let scope_name = scope_entry.file_name();
                    // Skip dot-prefixed entries (e.g., .DS_Store, .git)
                    // matching tsc behavior for type root discovery
                    if scope_name.to_str().is_some_and(|n| n.starts_with('.')) {
                        continue;
                    }
                    if count_is_dir(&scope_path) {
                        let scope_name = scope_entry.file_name();
                        let scope_name = scope_name.to_string_lossy();
                        if !scope_name.starts_with('.') {
                            packages.push(scope_path);
                        }
                    }
                }
            }
            continue;
        }
        packages.push(path);
    }

    packages
}

#[cfg(test)]
pub(crate) fn resolve_type_package_entry(
    package_root: &Path,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let mut cache = ModuleResolutionCache::default();
    resolve_type_package_entry_with_cache(package_root, options, &mut cache)
}

pub(crate) fn resolve_type_package_entry_with_cache(
    package_root: &Path,
    options: &ResolvedCompilerOptions,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let package_json = resolution_cache.read_package_json(&package_root.join("package.json"));

    // In node10/classic module resolution, type package fallback resolution
    // should NOT try .d.mts/.d.cts extensions (those require exports map).
    // Only bundler/node16/nodenext try the full extension set.
    let use_restricted_extensions = matches!(
        options.effective_module_resolution(),
        ModuleResolutionKind::Node | ModuleResolutionKind::Classic
    );

    if use_restricted_extensions {
        // Use restricted resolution: only types/typings/main + index.d.ts fallback
        let mut candidates = Vec::new();
        if let Some(ref pj) = package_json {
            candidates = collect_package_entry_candidates(&pj);
        }
        if !candidates
            .iter()
            .any(|entry| entry == "index" || entry == "./index")
        {
            candidates.push("index".to_string());
        }
        // Only try .ts, .tsx, .d.ts extensions (no .d.mts/.d.cts)
        let restricted_extensions = &["ts", "tsx", "d.ts"];
        for entry_name in candidates {
            let entry_name = entry_name.trim().trim_start_matches("./");
            let path = package_root.join(entry_name);
            for ext in restricted_extensions {
                let candidate = path.with_extension(ext);
                if resolution_cache.file_exists(&candidate) && is_declaration_file(&candidate) {
                    return Some(normalize_resolved_path(&candidate, options));
                }
            }
        }
        None
    } else {
        // For bundler/node16/nodenext, use resolve_package_specifier which respects
        // the exports map. This is needed for type packages that use conditional exports
        // (e.g. `"exports": { ".": { "import": "./index.d.mts", "require": "./index.d.cts" } }`)
        let conditions = package_lookup::export_conditions(options);
        let resolved = package_lookup::resolve_package_specifier(
            package_root,
            None,
            package_json.as_ref(),
            &conditions,
            options,
            resolution_cache,
        )?;
        is_declaration_file(&resolved).then_some(resolved)
    }
}

/// Resolve a type package entry using a specific resolution-mode condition.
///
/// When `resolution_mode` is "import" or "require", the exports map is consulted
/// with the corresponding condition. This implements the `resolution-mode` attribute
/// of `/// <reference types="..." resolution-mode="..." />` directives.
#[cfg(test)]
pub(crate) fn resolve_type_package_entry_with_mode(
    package_root: &Path,
    resolution_mode: &str,
    options: &ResolvedCompilerOptions,
) -> Option<PathBuf> {
    let mut cache = ModuleResolutionCache::default();
    resolve_type_package_entry_with_mode_and_cache(
        package_root,
        resolution_mode,
        options,
        &mut cache,
    )
}

pub(crate) fn resolve_type_package_entry_with_mode_and_cache(
    package_root: &Path,
    resolution_mode: &str,
    options: &ResolvedCompilerOptions,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let package_json = resolution_cache.read_package_json(&package_root.join("package.json"));
    let package_json = package_json.as_ref()?;

    // Build conditions based on resolution mode
    let conditions: Vec<&str> = match resolution_mode {
        "require" => vec!["require", "types", "default"],
        "import" => vec!["import", "types", "default"],
        _ => return None,
    };

    // Try the exports map first
    let compiler_version = package_lookup::types_versions_compiler_version(options);
    if let Some(exports) = &package_json.exports
        && let Some(target) =
            package_lookup::resolve_exports_subpath(exports, ".", &conditions, compiler_version)
        && let Some(target_path) = package_relative_target_path(package_root, &target)
    {
        // Try to find a declaration file at the target
        let package_type = package_type_from_json(Some(package_json));
        for candidate in expand_module_path_candidates(&target_path, options, package_type) {
            if resolution_cache.file_exists(&candidate) && is_declaration_file(&candidate) {
                return Some(normalize_resolved_path(&candidate, options));
            }
        }
        // Try exact path
        if resolution_cache.file_exists(&target_path) && is_declaration_file(&target_path) {
            return Some(normalize_resolved_path(&target_path, options));
        }
    }

    None
}

pub(crate) fn default_type_roots(base_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = FxHashSet::default();
    let mut current = Some(base_dir.to_path_buf());

    while let Some(dir) = current {
        let candidate = dir.join("node_modules").join("@types");
        if count_is_dir(&candidate) {
            let canonical = canonicalize_or_owned(&candidate);
            if seen.insert(canonical.clone()) {
                roots.push(canonical);
            }
        }
        // tsc scopes implicit typeRoots to the project's tsconfig boundary —
        // once we hit a directory that hosts a `tsconfig.json`, further
        // ancestors are outside the project and their `@types` packages must
        // not be silently discovered. Without this, tests that declare
        // `declare module "xyz"` in a higher-level `node_modules/@types`
        // resolve the module when tsc correctly reports TS2307.
        if count_is_file(&dir.join("tsconfig.json")) {
            break;
        }
        current = dir.parent().map(Path::to_path_buf);
    }

    roots
}

#[path = "resolution_specifier_scanning.rs"]
mod specifier_scanning;
#[cfg(test)]
pub(crate) use specifier_scanning::collect_module_specifiers;
pub(crate) use specifier_scanning::{
    collect_export_binding_nodes, collect_import_bindings, collect_module_requests_from_text,
    collect_module_specifiers_for_check, collect_star_export_specifiers,
    json_type_attribute_enables_json_module, module_specifier_has_type_json_import_attribute,
};

#[derive(Clone, Debug, Deserialize)]
struct PackageJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    types: Option<String>,
    #[serde(default)]
    typings: Option<String>,
    #[serde(default)]
    main: Option<String>,
    #[serde(default)]
    module: Option<String>,
    #[serde(default, rename = "type")]
    package_type: Option<String>,
    #[serde(default)]
    exports: Option<serde_json::Value>,
    #[serde(default)]
    imports: Option<serde_json::Value>,
    #[serde(default, rename = "typesVersions")]
    types_versions: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SemVer {
    major: u32,
    minor: u32,
    patch: u32,
}

impl SemVer {
    const ZERO: Self = Self {
        major: 0,
        minor: 0,
        patch: 0,
    };
}

fn package_type_from_json(package_json: Option<&PackageJson>) -> Option<PackageType> {
    let package_json = package_json?;

    match package_json.package_type.as_deref() {
        Some("module") => Some(PackageType::Module),
        Some("commonjs") | None => Some(PackageType::CommonJs),
        Some(_) => None,
    }
}

fn read_package_json_uncached(path: &Path) -> Option<PackageJson> {
    // PERF: see `docs/plan/PERFORMANCE_PLAN.md`. Resolver hot path
    // — package.json reads dominate sample profiles on full large-ts-repo.
    count_read_package_json();
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn collect_package_entry_candidates(package_json: &PackageJson) -> Vec<String> {
    let mut seen = FxHashSet::default();
    let mut candidates = Vec::new();

    for value in [package_json.types.as_ref(), package_json.typings.as_ref()]
        .into_iter()
        .flatten()
    {
        if seen.insert(value.clone()) {
            candidates.push(value.clone());
        }
    }

    for value in [package_json.module.as_ref(), package_json.main.as_ref()]
        .into_iter()
        .flatten()
    {
        if seen.insert(value.clone()) {
            candidates.push(value.clone());
        }
    }

    candidates
}

const KNOWN_EXTENSIONS: [&str; 12] = [
    ".d.mts", ".d.cts", ".d.ts", ".mts", ".cts", ".tsx", ".ts", ".mjs", ".cjs", ".jsx", ".js",
    ".json",
];
const TS_EXTENSION_CANDIDATES: [&str; 7] = ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];
const PACKAGE_INDEX_FALLBACK_EXTENSIONS: [&str; 3] = ["ts", "tsx", "d.ts"];
const PACKAGE_INDEX_FALLBACK_ALLOW_JS_EXTENSIONS: [&str; 5] = ["ts", "tsx", "d.ts", "js", "jsx"];

const NODE16_MODULE_EXTENSION_CANDIDATES: [&str; 7] =
    ["mts", "d.mts", "ts", "tsx", "d.ts", "cts", "d.cts"];
const NODE16_COMMONJS_EXTENSION_CANDIDATES: [&str; 7] =
    ["cts", "d.cts", "ts", "tsx", "d.ts", "mts", "d.mts"];

#[path = "resolution_package_lookup.rs"]
mod package_lookup;

pub(crate) fn resolve_module_specifier(
    from_file: &Path,
    module_specifier: &str,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    resolution_cache: &mut ModuleResolutionCache,
    known_files: &FxHashSet<PathBuf>,
) -> Option<PathBuf> {
    let debug = std::env::var_os("TSZ_DEBUG_RESOLVE").is_some();
    if debug {
        tracing::debug!(
            "resolve_module_specifier: from_file={from_file:?}, specifier={module_specifier:?}, resolution={:?}, base_url={:?}",
            options.effective_module_resolution(),
            options.base_url
        );
    }
    let specifier = module_specifier.trim();
    if specifier.is_empty() {
        return None;
    }
    let specifier = specifier.replace('\\', "/");
    let resolution = options.effective_module_resolution();
    if specifier.starts_with('#') {
        if package_lookup::is_invalid_package_import_specifier(&specifier, resolution) {
            return None;
        }
        if options.resolve_package_json_imports {
            return package_lookup::resolve_package_imports_specifier(
                from_file,
                &specifier,
                base_dir,
                options,
                resolution_cache,
            );
        }
        return None;
    }
    let mut candidates = Vec::new();

    let from_dir = from_file.parent().unwrap_or(base_dir);
    let package_type = match resolution {
        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
            resolution_cache.package_type_for_dir(from_dir, base_dir)
        }
        _ => None,
    };

    let mut allow_node_modules = false;
    let mut path_mapping_attempted = false;

    if Path::new(&specifier).is_absolute() {
        candidates.extend(expand_module_path_candidates(
            &PathBuf::from(specifier.as_str()),
            options,
            package_type,
        ));
    } else if is_path_relative(&specifier) {
        let joined = from_dir.join(&specifier);
        candidates.extend(expand_module_path_candidates(
            &joined,
            options,
            package_type,
        ));
        for candidate in root_dirs_relative_candidates(from_dir, &specifier, options) {
            candidates.extend(expand_module_path_candidates(
                &candidate,
                options,
                package_type,
            ));
        }
    } else if matches!(resolution, ModuleResolutionKind::Classic) {
        if let Some(paths) = options.paths.as_ref()
            && let Some((mapping, wildcard)) =
                resolution_cache.select_path_mapping(paths, &specifier)
        {
            path_mapping_attempted = true;
            let base = options.base_url.as_deref().unwrap_or(base_dir);
            for target in &mapping.targets {
                let substituted = substitute_path_target(target, &wildcard);
                let path = if Path::new(&substituted).is_absolute() {
                    PathBuf::from(substituted)
                } else {
                    base.join(substituted)
                };
                candidates.extend(expand_module_path_candidates(&path, options, package_type));
            }
        }

        // Classic resolution always walks up the directory tree from the containing
        // file's directory, probing for <specifier>.ts/.tsx/.d.ts and related candidates.
        // This runs even when baseUrl/path-mapping candidates were generated, matching
        // TypeScript behavior where classic resolution falls back to relative ancestor checks.
        // Unlike Node resolution, Classic resolution walks up for all specifiers including
        // bare module specifiers (e.g., "module3") since it has no node_modules concept.
        {
            let mut current = from_dir.to_path_buf();
            loop {
                candidates.extend(expand_module_path_candidates(
                    &current.join(&specifier),
                    options,
                    package_type,
                ));

                match current.parent() {
                    Some(parent) if parent != current => current = parent.to_path_buf(),
                    _ => break,
                }
            }
        }
    } else {
        allow_node_modules = true;
        if let Some(paths) = options.paths.as_ref()
            && let Some((mapping, wildcard)) =
                resolution_cache.select_path_mapping(paths, &specifier)
        {
            path_mapping_attempted = true;
            let base = options.base_url.as_deref().unwrap_or(base_dir);
            for target in &mapping.targets {
                let substituted = substitute_path_target(target, &wildcard);
                let path = if Path::new(&substituted).is_absolute() {
                    PathBuf::from(substituted)
                } else {
                    base.join(substituted)
                };
                candidates.extend(expand_module_path_candidates(&path, options, package_type));
            }
        }

        if candidates.is_empty()
            && let Some(base_url) = options.base_url.as_ref()
        {
            candidates.extend(expand_module_path_candidates(
                &base_url.join(&specifier),
                options,
                package_type,
            ));
        }
    }

    for candidate in candidates {
        // Check if candidate exists in known files (for virtual test files) or on filesystem
        let exists = known_files.contains(&candidate)
            || (resolution_cache.file_exists(&candidate) && is_valid_module_or_js_file(&candidate));
        if debug {
            tracing::debug!("candidate={candidate:?} exists={exists}");
        }

        if exists {
            return Some(normalize_resolved_path(&candidate, options));
        }
    }

    // TypeScript falls through to Classic-style directory walking when path mappings
    // were attempted but did not resolve. This matches behavior where path mapping
    // misses are not treated as terminal failures in classic mode.
    if path_mapping_attempted && matches!(resolution, ModuleResolutionKind::Classic) {
        let mut current = from_dir.to_path_buf();
        loop {
            for candidate in
                expand_module_path_candidates(&current.join(&specifier), options, package_type)
            {
                let exists = known_files.contains(&candidate)
                    || (resolution_cache.file_exists(&candidate)
                        && is_valid_module_or_js_file(&candidate));
                if debug {
                    tracing::debug!("classic-fallback candidate={candidate:?} exists={exists}");
                }
                if exists {
                    return Some(normalize_resolved_path(&candidate, options));
                }
            }

            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }
    }

    if allow_node_modules {
        return package_lookup::resolve_node_module_specifier(
            from_file,
            &specifier,
            base_dir,
            options,
            resolution_cache,
        );
    }

    None
}

fn root_dirs_relative_candidates(
    from_dir: &Path,
    specifier: &str,
    options: &ResolvedCompilerOptions,
) -> Vec<PathBuf> {
    if options.root_dirs.is_empty() {
        return Vec::new();
    }

    let from_dir = normalize_path(from_dir);
    let direct_candidate = normalize_path(&from_dir.join(specifier));
    let mut candidates = Vec::new();

    for origin_root in &options.root_dirs {
        let origin_root = normalize_path(origin_root);
        if from_dir.strip_prefix(&origin_root).is_err() {
            continue;
        }
        let Ok(virtual_path) = direct_candidate.strip_prefix(&origin_root) else {
            continue;
        };

        for target_root in &options.root_dirs {
            let candidate = normalize_path(&target_root.join(virtual_path));
            if candidate == direct_candidate || candidates.iter().any(|seen| seen == &candidate) {
                continue;
            }
            count_candidate_path();
            candidates.push(candidate);
        }
    }

    candidates
}

fn select_path_mapping(mappings: &[PathMapping], specifier: &str) -> Option<(usize, String)> {
    // `build_path_mappings` sorts by TypeScript precedence:
    // longest prefix, then longest pattern, then lexical pattern. The first
    // match is therefore the best match.
    for (idx, mapping) in mappings.iter().enumerate() {
        let Some(wildcard) = mapping.match_specifier(specifier) else {
            continue;
        };
        return Some((idx, wildcard));
    }

    None
}

fn substitute_path_target(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replace('*', wildcard)
    } else {
        target.to_string()
    }
}

fn expand_module_path_candidates(
    path: &Path,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Vec<PathBuf> {
    let base = normalize_path(path);
    let mut default_suffixes: Vec<String> = Vec::new();
    let suffixes = if options.module_suffixes.is_empty() {
        default_suffixes.push(String::new());
        &default_suffixes
    } else {
        &options.module_suffixes
    };
    if let Some((base_no_ext, extension)) = split_path_extension(&base) {
        // Try extension substitution (.js → .ts/.tsx/.d.ts) for all resolution modes.
        // TypeScript resolves `.js` imports to `.ts` sources in all modes.
        let mut candidates = Vec::new();
        if let Some(rewritten) = node16_extension_substitution(&base, extension) {
            for candidate in rewritten {
                candidates.extend(candidates_with_suffixes(&candidate, suffixes));
            }
        }
        // Also include the original extension as fallback
        candidates.extend(candidates_with_suffixes_and_extension(
            &base_no_ext,
            extension,
            suffixes,
        ));
        return candidates;
    }

    let extensions = extension_candidates_for_resolution(options, package_type);
    let mut candidates = Vec::new();
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(&base, ext, suffixes));
    }
    let index = base.join("index");
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(
            &index, ext, suffixes,
        ));
    }
    candidates
}

fn expand_export_path_candidates(
    path: &Path,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> Vec<PathBuf> {
    let base = normalize_path(path);
    let suffixes = &options.module_suffixes;
    if let Some((base_no_ext, extension)) = split_path_extension(&base) {
        // Package `exports` targets participate in declaration-sidecar lookup
        // during program discovery. This keeps the driver aligned with the
        // checker `ModuleResolver`, which resolves `./entry.js` to adjacent
        // `./entry.d.ts` / `./entry.d.mts` / `./entry.d.cts` files when those
        // are the type-bearing program inputs.
        let mut candidates = Vec::new();
        if let Some(rewritten) = node16_extension_substitution(&base, extension) {
            for candidate in rewritten {
                candidates.extend(candidates_with_suffixes(&candidate, suffixes));
            }
        }
        candidates.extend(candidates_with_suffixes_and_extension(
            &base_no_ext,
            extension,
            suffixes,
        ));
        return candidates;
    }

    let extensions = extension_candidates_for_resolution(options, package_type);
    let mut candidates = Vec::new();
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(&base, ext, suffixes));
    }
    if options.resolve_json_module {
        candidates.extend(candidates_with_suffixes_and_extension(
            &base, "json", suffixes,
        ));
    }
    let index = base.join("index");
    for ext in extensions {
        candidates.extend(candidates_with_suffixes_and_extension(
            &index, ext, suffixes,
        ));
    }
    if options.resolve_json_module {
        candidates.extend(candidates_with_suffixes_and_extension(
            &index, "json", suffixes,
        ));
    }
    candidates
}

fn split_path_extension(path: &Path) -> Option<(PathBuf, &'static str)> {
    let path_str = path.to_string_lossy();
    for ext in KNOWN_EXTENSIONS {
        if path_str.ends_with(ext) {
            let base = &path_str[..path_str.len().saturating_sub(ext.len())];
            if base.is_empty() {
                return None;
            }
            return Some((PathBuf::from(base), ext.trim_start_matches('.')));
        }
    }
    None
}

fn candidates_with_suffixes(path: &Path, suffixes: &[String]) -> Vec<PathBuf> {
    let Some((base, extension)) = split_path_extension(path) else {
        return Vec::new();
    };
    candidates_with_suffixes_and_extension(&base, extension, suffixes)
}

fn candidates_with_suffixes_and_extension(
    base: &Path,
    extension: &str,
    suffixes: &[String],
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for suffix in suffixes {
        if let Some(candidate) = path_with_suffix_and_extension(base, suffix, extension) {
            count_candidate_path();
            candidates.push(candidate);
        }
    }
    candidates
}

fn path_with_suffix_and_extension(base: &Path, suffix: &str, extension: &str) -> Option<PathBuf> {
    let file_name = base.file_name()?.to_string_lossy();
    let mut candidate = base.to_path_buf();
    let mut new_name = String::with_capacity(file_name.len() + suffix.len() + extension.len() + 1);
    new_name.push_str(&file_name);
    new_name.push_str(suffix);
    new_name.push('.');
    new_name.push_str(extension);
    candidate.set_file_name(new_name);
    Some(candidate)
}

fn node16_extension_substitution(path: &Path, extension: &str) -> Option<Vec<PathBuf>> {
    let replacements: &[&str] = match extension {
        "js" => &["ts", "tsx", "d.ts"],
        "jsx" => &["tsx", "d.ts"],
        "mjs" => &["mts", "d.mts"],
        "cjs" => &["cts", "d.cts"],
        _ => return None,
    };

    Some(
        replacements
            .iter()
            .map(|ext| path.with_extension(ext))
            .collect(),
    )
}

const fn extension_candidates_for_resolution(
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> &'static [&'static str] {
    match options.effective_module_resolution() {
        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => match package_type {
            Some(PackageType::Module) => &NODE16_MODULE_EXTENSION_CANDIDATES,
            Some(PackageType::CommonJs) => &NODE16_COMMONJS_EXTENSION_CANDIDATES,
            None => &TS_EXTENSION_CANDIDATES,
        },
        _ => &TS_EXTENSION_CANDIDATES,
    }
}

pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::RootDir
            | std::path::Component::Normal(_)
            | std::path::Component::Prefix(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

pub(crate) fn normalize_resolved_path(path: &Path, options: &ResolvedCompilerOptions) -> PathBuf {
    let normalized = normalize_path(path);
    if options.preserve_symlinks {
        normalized
    } else {
        let canonical = canonicalize_or_owned(path);
        let preserve_package_link_identity = path_has_symlinked_package_ancestor(path)
            || (!has_node_modules_component(path) && has_node_modules_component(&canonical));
        if preserve_package_link_identity {
            normalized
        } else {
            canonical
        }
    }
}

/// Find the innermost `node_modules/<package>/` root for a file path.
fn find_node_modules_package_root(path: &Path) -> Option<PathBuf> {
    let components: Vec<_> = path.components().collect();
    for i in (0..components.len()).rev() {
        if components[i].as_os_str() == "node_modules" && i + 1 < components.len() {
            let next = components[i + 1].as_os_str().to_string_lossy();
            let pkg_end = if next.starts_with('@') {
                if i + 2 < components.len() {
                    i + 3
                } else {
                    continue;
                }
            } else {
                i + 2
            };
            if pkg_end <= components.len() {
                let mut root = PathBuf::new();
                for c in &components[..pkg_end] {
                    root.push(c);
                }
                return Some(root);
            }
        }
    }
    None
}

fn path_has_symlinked_package_ancestor(path: &Path) -> bool {
    let mut current = path.parent();
    while let Some(dir) = current {
        if std::fs::symlink_metadata(dir)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            if is_root_alias_symlink(dir) {
                current = dir.parent();
                continue;
            }

            let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
            return canonical.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::Normal(part) if part.to_str() == Some("node_modules")
                )
            });
        }
        current = dir.parent();
    }
    false
}

fn has_node_modules_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            std::path::Component::Normal(part) if part.to_str() == Some("node_modules")
        )
    })
}

fn is_root_alias_symlink(dir: &Path) -> bool {
    if !dir.is_absolute() {
        return false;
    }

    let Ok(relative_to_root) = dir.strip_prefix(Path::new("/")) else {
        return false;
    };
    let Ok(canonical) = std::fs::canonicalize(dir) else {
        return false;
    };
    let Ok(canonical_relative_to_root) = canonical.strip_prefix(Path::new("/")) else {
        return false;
    };

    canonical_relative_to_root.ends_with(relative_to_root)
}

/// Build a redirect map for duplicate packages (same name+version at different
/// `node_modules` paths). The shallowest copy becomes canonical.
pub(crate) fn build_duplicate_package_redirects(
    file_names: &[String],
    options: &ResolvedCompilerOptions,
) -> FxHashMap<PathBuf, PathBuf> {
    use std::collections::hash_map::Entry;

    let mut canonical_packages: FxHashMap<(String, String), (PathBuf, usize)> =
        FxHashMap::default();
    let mut package_roots: FxHashSet<PathBuf> = FxHashSet::default();
    for file_name in file_names {
        if let Some(pkg_root) = find_node_modules_package_root(Path::new(file_name)) {
            package_roots.insert(pkg_root);
        }
    }

    for root in &package_roots {
        tracing::debug!(target: "tsz::dup_pkg", root = %root.display(), "found package root");
    }

    let mut root_redirects: FxHashMap<PathBuf, PathBuf> = FxHashMap::default();
    for pkg_root in &package_roots {
        let pkg_json_path = pkg_root.join("package.json");
        let (name, version) = match std::fs::read_to_string(&pkg_json_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(val) => {
                    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let version = val.get("version").and_then(|v| v.as_str()).unwrap_or("");
                    if name.is_empty() || version.is_empty() {
                        continue;
                    }
                    (name.to_string(), version.to_string())
                }
                Err(_) => continue,
            },
            Err(e) => {
                tracing::debug!(target: "tsz::dup_pkg", path = %pkg_json_path.display(), error = %e, "cannot read package.json");
                continue;
            }
        };
        tracing::debug!(target: "tsz::dup_pkg", name = %name, version = %version, root = %pkg_root.display(), "package found");
        let depth = pkg_root
            .components()
            .filter(|c| c.as_os_str() == "node_modules")
            .count();
        match canonical_packages.entry((name, version)) {
            Entry::Vacant(e) => {
                e.insert((pkg_root.clone(), depth));
            }
            Entry::Occupied(mut e) => {
                let (existing_root, existing_depth) = e.get().clone();
                let current_rank = (
                    depth,
                    normalize_resolved_path(pkg_root, options)
                        .to_string_lossy()
                        .into_owned(),
                );
                let existing_rank = (
                    existing_depth,
                    normalize_resolved_path(&existing_root, options)
                        .to_string_lossy()
                        .into_owned(),
                );
                if current_rank < existing_rank {
                    root_redirects.insert(existing_root, pkg_root.clone());
                    e.insert((pkg_root.clone(), depth));
                } else {
                    root_redirects.insert(pkg_root.clone(), existing_root);
                }
            }
        }
    }
    if root_redirects.is_empty() {
        return FxHashMap::default();
    }
    for (from, to) in &root_redirects {
        tracing::debug!(target: "tsz::dup_pkg", from = %from.display(), to = %to.display(), "root redirect");
    }
    let mut file_redirects: FxHashMap<PathBuf, PathBuf> = FxHashMap::default();
    for file_name in file_names {
        let file_path = Path::new(file_name);
        if let Some(pkg_root) = find_node_modules_package_root(file_path)
            && let Some(canonical_root) = root_redirects.get(&pkg_root)
            && let Ok(relative) = file_path.strip_prefix(&pkg_root)
        {
            let canonical_file = canonical_root.join(relative);
            let from = normalize_resolved_path(file_path, options);
            let to = normalize_resolved_path(&canonical_file, options);
            tracing::debug!(target: "tsz::dup_pkg", from = %from.display(), to = %to.display(), "file redirect");
            if from != to {
                file_redirects.insert(from, to);
            }
        }
    }
    file_redirects
}

pub(crate) fn is_declaration_file(path: &Path) -> bool {
    tsz::module_resolver::ModuleExtension::from_path(path).is_declaration()
}

pub(crate) fn canonicalize_with_missing_tail(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }

    let mut tail = Vec::new();
    let mut current = path;
    while !current.exists() {
        let Some(name) = current.file_name() else {
            return path.to_path_buf();
        };
        tail.push(name.to_os_string());
        let Some(parent) = current.parent() else {
            return path.to_path_buf();
        };
        current = parent;
    }

    let Ok(mut canonical) = std::fs::canonicalize(current) else {
        return path.to_path_buf();
    };
    for component in tail.iter().rev() {
        canonical.push(component);
    }
    canonical
}

pub(crate) fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let normalized = value.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
}

#[cfg(test)]
#[path = "resolution_tests.rs"]
mod resolution_tests;
