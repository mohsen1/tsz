use rustc_hash::FxHashSet;
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::config::{ModuleResolutionKind, ResolvedCompilerOptions};
use crate::fs::{is_valid_module_file, is_valid_module_or_js_file};
use tsz::emitter::ModuleKind;
use tsz::module_resolver::PackageType;

#[allow(unused_imports)]
use super::*;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct PackageJson {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) types: Option<String>,
    #[serde(default)]
    pub(super) typings: Option<String>,
    #[serde(default)]
    pub(super) main: Option<String>,
    #[serde(default)]
    pub(super) module: Option<String>,
    #[serde(default, rename = "type")]
    pub(super) package_type: Option<String>,
    #[serde(default)]
    pub(super) exports: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) imports: Option<serde_json::Value>,
    #[serde(default, rename = "typesVersions")]
    pub(super) types_versions: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct SemVer {
    pub(super) major: u32,
    pub(super) minor: u32,
    pub(super) patch: u32,
}

impl SemVer {
    pub(super) const ZERO: Self = Self {
        major: 0,
        minor: 0,
        patch: 0,
    };
}

// NOTE: Keep this in sync with the TypeScript version this compiler targets.
// TODO: Make this configurable once CLI plumbing is available.
pub(crate) const TYPES_VERSIONS_COMPILER_VERSION_FALLBACK: SemVer = SemVer {
    major: 6,
    minor: 0,
    patch: 3,
};

pub(crate) fn types_versions_compiler_version(options: &ResolvedCompilerOptions) -> SemVer {
    options
        .types_versions_compiler_version
        .as_deref()
        .and_then(parse_semver)
        .unwrap_or_else(default_types_versions_compiler_version)
}

pub(crate) const fn default_types_versions_compiler_version() -> SemVer {
    // Use the fallback version directly since the project's package.json version
    // is not a TypeScript version. The fallback represents the TypeScript version
    // that this compiler is compatible with for typesVersions resolution.
    TYPES_VERSIONS_COMPILER_VERSION_FALLBACK
}

pub(crate) fn export_conditions(options: &ResolvedCompilerOptions) -> Vec<&'static str> {
    let resolution = options.effective_module_resolution();
    let mut conditions = Vec::new();
    push_condition(&mut conditions, "types");

    // Per tsc 6.0, only Node-targeted resolution kinds get the `node`
    // condition by default. Bundler mode does NOT default to `browser`;
    // the user must opt in via `customConditions`.
    match resolution {
        ModuleResolutionKind::Bundler => {}
        ModuleResolutionKind::Classic
        | ModuleResolutionKind::Node
        | ModuleResolutionKind::Node16
        | ModuleResolutionKind::NodeNext => {
            push_condition(&mut conditions, "node");
        }
    }

    match options.printer.module {
        ModuleKind::CommonJS | ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System => {
            push_condition(&mut conditions, "require");
        }
        ModuleKind::ES2015
        | ModuleKind::ES2020
        | ModuleKind::ES2022
        | ModuleKind::ESNext
        | ModuleKind::Node16
        | ModuleKind::Node18
        | ModuleKind::Node20
        | ModuleKind::NodeNext => {
            push_condition(&mut conditions, "import");
        }
        _ => {}
    }

    push_condition(&mut conditions, "default");
    match resolution {
        ModuleResolutionKind::Bundler => {
            push_condition(&mut conditions, "import");
            push_condition(&mut conditions, "require");
        }
        ModuleResolutionKind::Classic
        | ModuleResolutionKind::Node
        | ModuleResolutionKind::Node16
        | ModuleResolutionKind::NodeNext => {
            push_condition(&mut conditions, "import");
            push_condition(&mut conditions, "require");
            push_condition(&mut conditions, "browser");
        }
    }

    conditions
}

pub(crate) fn push_condition(conditions: &mut Vec<&'static str>, condition: &'static str) {
    if !conditions.contains(&condition) {
        conditions.push(condition);
    }
}

/// Validates a relative `exports`/`imports` target string per Node.js
/// `PACKAGE_TARGET_RESOLVE`.
///
/// A valid relative target:
/// - Starts with `"./"`.
/// - Contains no `..` path segment (cannot escape the package root).
/// - Contains no `node_modules` path segment.
pub(crate) fn is_valid_relative_package_target(target: &str) -> bool {
    if !target.starts_with("./") {
        return false;
    }
    for segment in target.split('/') {
        if segment == ".." || segment == "node_modules" {
            return false;
        }
    }
    true
}

/// Validates a bare-specifier `imports` target. Bare targets must not be
/// empty and must not be absolute (Unix `/...`, Windows `\...`/drive paths).
pub(crate) fn is_valid_bare_imports_target(target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    if target.starts_with('/') || target.starts_with('\\') {
        return false;
    }
    if target.starts_with("./") || target.starts_with("../") {
        return false;
    }
    let bytes = target.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return false;
    }
    true
}

pub(crate) fn resolve_node_module_specifier(
    from_file: &Path,
    module_specifier: &str,
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let (package_name, subpath) = split_package_specifier(module_specifier)?;
    let conditions = export_conditions(options);

    // Self-reference: check if any ancestor package.json has a "name" matching
    // the import specifier. Node.js supports importing your own package by name
    // using the "exports" field in package.json.
    {
        let mut dir = from_file.parent().unwrap_or(base_dir);
        loop {
            let pj_path = dir.join("package.json");
            if let Some(pj) = resolution_cache.read_package_json(&pj_path) {
                if pj.name.as_deref() == Some(&package_name) {
                    let resolved = resolve_package_specifier(
                        dir,
                        subpath.as_deref(),
                        Some(&pj),
                        &conditions,
                        options,
                        resolution_cache,
                    );
                    if resolved.is_some() {
                        return resolved;
                    }

                    // Output-to-source remapping for self-reference imports.
                    // When outDir/declarationDir is set, export map targets point
                    // to the output directory (e.g., "./dist/index.js"). tsc
                    // remaps these back to source files by stripping the output
                    // prefix and substituting output extensions with source
                    // extensions (tryLoadInputFileForPath).
                    if let Some(ref exports) = pj.exports {
                        let subpath_key = match &subpath {
                            Some(value) => format!("./{value}"),
                            None => ".".to_string(),
                        };
                        if let Some(target) = resolve_exports_subpath(
                            exports,
                            &subpath_key,
                            &conditions,
                            types_versions_compiler_version(options),
                        ) && let Some(resolved) = try_remap_output_to_source(
                            dir,
                            &target,
                            from_file,
                            options,
                            resolution_cache,
                        ) {
                            return Some(resolved);
                        }
                    }
                }
                // Stop at the first package.json with a name (that's the package boundary)
                if pj.name.is_some() {
                    break;
                }
            }
            if dir == base_dir {
                break;
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }

    let mut current = from_file.parent().unwrap_or(base_dir);

    loop {
        // 1. Look for the package itself in node_modules
        let child_node_modules = current.join("node_modules");
        let mut node_modules_roots = Vec::new();
        if current
            .file_name()
            .is_some_and(|name| name == "node_modules")
        {
            node_modules_roots.push(current.to_path_buf());
        }
        if resolution_cache.node_modules_dir_exists(&child_node_modules) {
            node_modules_roots.push(child_node_modules);
        }

        for node_modules in node_modules_roots {
            let package_root = node_modules.join(&package_name);
            if resolution_cache.package_root_dir_exists(&package_root) {
                let package_json =
                    resolution_cache.read_package_json(&package_root.join("package.json"));
                let resolved = resolve_package_specifier(
                    &package_root,
                    subpath.as_deref(),
                    package_json.as_ref(),
                    &conditions,
                    options,
                    resolution_cache,
                );
                if resolved.is_some() {
                    return resolved;
                }
            } else if subpath.is_none()
                && options.effective_module_resolution() == ModuleResolutionKind::Bundler
            {
                let candidates = expand_module_path_candidates(&package_root, options, None);
                for candidate in candidates {
                    if resolution_cache.file_exists(&candidate)
                        && is_valid_module_or_js_file(&candidate)
                    {
                        return Some(normalize_resolved_path(&candidate, options));
                    }
                }
            }

            // 2. Look for @types package (if not already looking for one)
            // TypeScript looks up @types/foo for 'foo', and @types/scope__pkg for '@scope/pkg'
            if !options.checker.no_types_and_symbols && !package_name.starts_with("@types/") {
                let types_package_name = if let Some(scope_pkg) = package_name.strip_prefix('@') {
                    // Scoped package: @scope/pkg -> @types/scope__pkg
                    // Skip the '@' (1 char) and replace '/' with '__'
                    format!("@types/{}", scope_pkg.replace('/', "__"))
                } else {
                    format!("@types/{package_name}")
                };

                let types_root = node_modules.join(&types_package_name);
                if resolution_cache.package_root_dir_exists(&types_root) {
                    let package_json =
                        resolution_cache.read_package_json(&types_root.join("package.json"));
                    let resolved = resolve_package_specifier(
                        &types_root,
                        subpath.as_deref(),
                        package_json.as_ref(),
                        &conditions,
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

    // When a package was loaded through `types`/`typeRoots`, TypeScript still
    // treats bare imports from that package as resolved. Mirror that here by
    // consulting the configured type roots for package entrypoints after the
    // normal node_modules walk-up fails.
    if !options.checker.no_types_and_symbols && subpath.is_none() {
        let type_roots = options
            .type_roots
            .clone()
            .unwrap_or_else(|| default_type_roots(base_dir));
        if let Some(resolved) = resolve_type_package_from_roots_with_cache(
            &package_name,
            &type_roots,
            options,
            resolution_cache,
        ) {
            return Some(resolved);
        }
    }

    None
}

pub(crate) fn resolve_package_imports_specifier(
    from_file: &Path,
    module_specifier: &str,
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let conditions = export_conditions(options);
    let compiler_version = types_versions_compiler_version(options);
    let mut current = from_file.parent().unwrap_or(base_dir);

    loop {
        let package_json_path = current.join("package.json");
        if let Some(package_json) = resolution_cache.read_package_json(&package_json_path)
            && let Some(imports) = package_json.imports.as_ref()
        {
            let package_type = package_type_from_json(Some(&package_json));
            for target in resolve_imports_subpath_candidates(
                imports,
                module_specifier,
                &conditions,
                compiler_version,
            ) {
                let target = target.trim();
                if target.starts_with("./") {
                    if package_relative_target_path(current, target).is_none() {
                        continue;
                    }
                } else if !is_valid_bare_imports_target(target) {
                    continue;
                }
                if let Some(resolved) =
                    resolve_package_entry(current, target, options, package_type, resolution_cache)
                {
                    return Some(resolved);
                }
                // Output-to-source remapping for package imports.
                // When outDir/declarationDir is set, import targets like "./dist/index.js"
                // point to the output directory which doesn't exist at compile time.
                // Remap back to source files (e.g., "./index.ts").
                if let Some(resolved) = try_remap_output_to_source(
                    current,
                    target,
                    from_file,
                    options,
                    resolution_cache,
                ) {
                    return Some(resolved);
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

pub(crate) fn is_invalid_package_import_specifier(
    specifier: &str,
    resolution: ModuleResolutionKind,
) -> bool {
    if specifier == "#" {
        return true;
    }
    // In node16 module resolution, #/ prefixed specifiers are invalid.
    // In nodenext (and bundler), they can match wildcard patterns like "#/*".
    if specifier.starts_with("#/") && resolution == ModuleResolutionKind::Node16 {
        return true;
    }
    false
}

pub(crate) fn resolve_package_specifier(
    package_root: &Path,
    subpath: Option<&str>,
    package_json: Option<&PackageJson>,
    conditions: &[&str],
    options: &ResolvedCompilerOptions,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let package_type = package_type_from_json(package_json);
    if let Some(package_json) = package_json {
        let has_exports = options.resolve_package_json_exports && package_json.exports.is_some();

        if has_exports {
            let exports = package_json
                .exports
                .as_ref()
                .expect("has_exports guard ensures exports is Some");
            let subpath_key = match subpath {
                Some(value) => format!("./{value}"),
                None => ".".to_string(),
            };
            if let Some(target) = resolve_exports_subpath(
                exports,
                &subpath_key,
                conditions,
                types_versions_compiler_version(options),
            ) && let Some(resolved) = resolve_export_entry(
                package_root,
                &target,
                options,
                package_type,
                resolution_cache,
            ) {
                return Some(resolved);
            }
            if let Some(types_versions) = package_json.types_versions.as_ref() {
                let types_subpath = subpath.unwrap_or("index");
                if let Some(resolved) = resolve_types_versions(
                    package_root,
                    types_subpath,
                    types_versions,
                    options,
                    package_type,
                    resolution_cache,
                ) {
                    return Some(resolved);
                }
            }
            // When an "exports" field exists and neither exports nor
            // typesVersions provide a match, treat it as unresolved.
            return None;
        }

        if let Some(types_versions) = package_json.types_versions.as_ref() {
            let types_subpath = subpath.unwrap_or("index");
            if let Some(resolved) = resolve_types_versions(
                package_root,
                types_subpath,
                types_versions,
                options,
                package_type,
                resolution_cache,
            ) {
                return Some(resolved);
            }
        }
    }

    if let Some(subpath) = subpath {
        return resolve_package_entry(
            package_root,
            subpath,
            options,
            package_type,
            resolution_cache,
        );
    }

    resolve_package_root(
        package_root,
        package_json,
        options,
        package_type,
        resolution_cache,
    )
}

pub(crate) fn split_package_specifier(specifier: &str) -> Option<(String, Option<String>)> {
    let mut parts = specifier.split('/');
    let first = parts.next()?;

    if first.starts_with('@') {
        let second = parts.next()?;
        let package = format!("{first}/{second}");
        let rest = parts.collect::<Vec<_>>().join("/");
        let subpath = if rest.is_empty() { None } else { Some(rest) };
        return Some((package, subpath));
    }

    let rest = parts.collect::<Vec<_>>().join("/");
    let subpath = if rest.is_empty() { None } else { Some(rest) };
    Some((first.to_string(), subpath))
}

pub(crate) fn resolve_package_root(
    package_root: &Path,
    package_json: Option<&PackageJson>,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    if let Some(package_json) = package_json {
        for entry in [package_json.types.as_ref(), package_json.typings.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(resolved) = resolve_declaration_package_entry(
                package_root,
                entry,
                options,
                package_type,
                resolution_cache,
            ) {
                return Some(resolved);
            }
        }

        for entry in [package_json.module.as_ref(), package_json.main.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(resolved) =
                resolve_package_entry(package_root, entry, options, package_type, resolution_cache)
            {
                return Some(resolved);
            }
        }
    }

    // Try index file fallback.
    //
    // For symlinked package roots with an explicit package.json, keep requiring an
    // explicit entry point (exports/main/types). But for symlinked roots without
    // package.json, allow index fallback (matches tsc's module resolution behavior
    // used by declaration-emit symlink conformance cases).
    let is_symlinked_package_root = std::fs::symlink_metadata(package_root)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false);
    let has_package_json = package_json.is_some();
    if (!is_symlinked_package_root || !has_package_json)
        && let Some(resolved) =
            resolve_package_index_fallback(package_root, options, resolution_cache)
    {
        return Some(resolved);
    }

    None
}

pub(crate) fn resolve_package_index_fallback(
    package_root: &Path,
    options: &ResolvedCompilerOptions,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let extensions = if options.allow_js {
        PACKAGE_INDEX_FALLBACK_ALLOW_JS_EXTENSIONS.as_slice()
    } else {
        PACKAGE_INDEX_FALLBACK_EXTENSIONS.as_slice()
    };
    let mut default_suffixes: Vec<String> = Vec::new();
    let suffixes = if options.module_suffixes.is_empty() {
        default_suffixes.push(String::new());
        &default_suffixes
    } else {
        &options.module_suffixes
    };
    let index = package_root.join("index");

    for ext in extensions {
        for candidate in candidates_with_suffixes_and_extension(&index, ext, suffixes) {
            if resolution_cache.file_exists(&candidate) && is_valid_module_or_js_file(&candidate) {
                return Some(normalize_resolved_path(&candidate, options));
            }
        }
    }

    None
}

pub(crate) fn resolve_declaration_package_entry(
    package_root: &Path,
    entry: &str,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    let entry = entry.trim_start_matches("./");
    let path = if Path::new(entry).is_absolute() {
        PathBuf::from(entry)
    } else {
        package_root.join(entry)
    };

    for candidate in expand_module_path_candidates(&path, options, package_type) {
        if resolution_cache.file_exists(&candidate) && is_declaration_file(&candidate) {
            return Some(normalize_resolved_path(&candidate, options));
        }
    }

    if resolution_cache.file_exists(&path) && is_declaration_file(&path) {
        return Some(normalize_resolved_path(&path, options));
    }

    if count_is_dir(&path)
        && let Some(pj) = resolution_cache.read_package_json(&path.join("package.json"))
    {
        let sub_type = package_type_from_json(Some(&pj));
        for entry in [pj.types.as_ref(), pj.typings.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(resolved) =
                resolve_declaration_package_entry(&path, entry, options, sub_type, resolution_cache)
            {
                return Some(resolved);
            }
        }
    }

    None
}

pub(crate) fn resolve_package_entry(
    package_root: &Path,
    entry: &str,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    let entry = entry.trim_start_matches("./");
    let path = if Path::new(entry).is_absolute() {
        PathBuf::from(entry)
    } else {
        package_root.join(entry)
    };

    // resolve_package_entry is used for `imports` field targets and `main` field
    // resolution — contexts where tsc accepts JS files as valid resolution targets
    // (they get added to the program via import-following). This differs from
    // resolve_export_entry which uses is_valid_module_file (TS/JSON only).
    //
    // In Node16/NodeNext with ESM packages (type: "module"), Node.js does not
    // perform directory index resolution. Skip index candidates for ESM packages.
    let is_esm_no_index = matches!(package_type, Some(PackageType::Module))
        && matches!(
            options.effective_module_resolution(),
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
        );
    for candidate in expand_module_path_candidates(&path, options, package_type) {
        // Skip directory index candidates (path/index.{ext}) for ESM packages
        if is_esm_no_index
            && candidate.parent() == Some(&path)
            && let Some(name) = candidate.file_name().and_then(|n| n.to_str())
            && name.starts_with("index.")
        {
            continue;
        }
        if resolution_cache.file_exists(&candidate) && is_valid_module_or_js_file(&candidate) {
            return Some(normalize_resolved_path(&candidate, options));
        }
    }

    // Check subpath's package.json for types/main fields
    if !is_esm_no_index
        && count_is_dir(&path)
        && let Some(pj) = resolution_cache.read_package_json(&path.join("package.json"))
    {
        let sub_type = package_type_from_json(Some(&pj));
        // Try types/typings field
        for types in [pj.types.as_ref(), pj.typings.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(resolved) =
                resolve_declaration_package_entry(&path, types, options, sub_type, resolution_cache)
            {
                return Some(resolved);
            }
        }
        // Try main field
        if let Some(main) = &pj.main {
            let main_path = path.join(main);
            for candidate in expand_module_path_candidates(&main_path, options, sub_type) {
                if resolution_cache.file_exists(&candidate)
                    && is_valid_module_or_js_file(&candidate)
                {
                    return Some(normalize_resolved_path(&candidate, options));
                }
            }
        }
    }

    None
}

pub(crate) fn resolve_export_entry(
    package_root: &Path,
    entry: &str,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let entry = entry.trim();
    if !is_valid_relative_package_target(entry) {
        // Per Node.js PACKAGE_TARGET_RESOLVE, exports targets must be
        // relative `./...` paths within the package root and must not
        // contain `..` or `node_modules` segments. Absolute paths and
        // parent escapes are rejected.
        return None;
    }
    let path = package_relative_target_path(package_root, entry)?;

    for candidate in expand_export_path_candidates(&path, options, package_type) {
        if resolution_cache.file_exists(&candidate) && is_valid_module_file(&candidate) {
            return Some(normalize_resolved_path(&candidate, options));
        }
    }

    None
}

/// Remap an export map target from the output directory to the source directory.
///
/// When `outDir` or `declarationDir` is set, export targets like `./dist/index.js`
/// point to the output directory which doesn't exist at compile time. tsc's
/// `tryLoadInputFileForPath` handles this by stripping the output directory prefix
/// and substituting output extensions (.js, .d.ts) with source extensions (.ts, .tsx).
///
/// Example: outDir="./dist", target="./dist/index.js"
///   → strip "./dist" → "index.js" → try "index.ts" → found!
pub(crate) fn try_remap_output_to_source(
    package_root: &Path,
    target: &str,
    _from_file: &Path,
    options: &ResolvedCompilerOptions,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    fn resolve_configured_path_against_package_root(
        configured: &Path,
        package_root: &Path,
        canon_package_root: &Path,
        _from_file: &Path,
        options: &ResolvedCompilerOptions,
    ) -> PathBuf {
        if configured.is_absolute() {
            if let Ok(relative) = configured.strip_prefix(package_root) {
                return canon_package_root.join(relative);
            }

            let canonical = normalize_resolved_path(configured, options);
            if canonical.exists() {
                return canonical;
            }

            // Conformance tests use virtual absolute paths like `/pkg/src`
            // while writing files under `<tmpdir>/pkg/src`. Re-anchor those
            // option paths to the temporary project root when the host-absolute
            // path doesn't exist.
            if let Some(project_root) = canon_package_root.parent()
                && let Ok(relative) = configured.strip_prefix(Path::new("/"))
            {
                let matches_package_root =
                    relative
                        .components()
                        .next()
                        .and_then(|component| match component {
                            std::path::Component::Normal(name) => Some(name),
                            _ => None,
                        })
                        == package_root.file_name();

                if matches_package_root {
                    return project_root.join(relative);
                }
            }

            return canonical;
        }

        canon_package_root.join(configured)
    }

    // Canonicalize package_root first (it exists) so that symlinks are resolved
    // before joining the target (which may not exist on disk).
    let canon_root = normalize_resolved_path(package_root, options);
    let target_path = package_relative_target_path(&canon_root, target)?;

    // Compute the source directory: the root from which source files are organized.
    // Use rootDir if set (already canonicalized), otherwise fall back to the
    // package root (where package.json lives). tsc uses getCommonSourceDirectory()
    // which defaults to the requesting file's directory for single-file projects,
    // but for self-reference resolution the package root is the correct fallback
    // since export targets are relative to it.
    let source_dir_owned;
    let source_dir = if let Some(ref root_dir) = options.root_dir {
        source_dir_owned = resolve_configured_path_against_package_root(
            root_dir,
            package_root,
            &canon_root,
            _from_file,
            options,
        );
        source_dir_owned.as_path()
    } else {
        source_dir_owned = canon_root.clone();
        source_dir_owned.as_path()
    };

    let out_dirs: Vec<&Path> = [
        options.out_dir.as_deref(),
        options.declaration_dir.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();

    if out_dirs.is_empty() {
        return None;
    }

    for out_dir in &out_dirs {
        let resolved_out_dir = resolve_configured_path_against_package_root(
            out_dir,
            package_root,
            &canon_root,
            _from_file,
            options,
        );

        // Check if the target path falls inside the output directory.
        let target_canon = normalize_path(&target_path);
        let out_canon = normalize_path(&resolved_out_dir);

        if let Ok(relative) = target_canon.strip_prefix(&out_canon) {
            // Target is inside the output dir. Build the source path.
            let source_base = source_dir.join(relative);

            // Try substituting output extensions with source extensions
            let source_exts: &[(&str, &[&str])] = &[
                (".js", &[".ts", ".tsx"]),
                (".jsx", &[".tsx", ".ts"]),
                (".mjs", &[".mts"]),
                (".cjs", &[".cts"]),
                (".d.ts", &[".ts", ".tsx"]),
                (".d.mts", &[".mts"]),
                (".d.cts", &[".cts"]),
            ];

            let source_str = source_base.to_string_lossy();
            for (out_ext, src_exts) in source_exts {
                if let Some(base) = source_str.strip_suffix(out_ext) {
                    for src_ext in *src_exts {
                        let candidate = PathBuf::from(format!("{base}{src_ext}"));
                        if resolution_cache.file_exists(&candidate) {
                            return Some(normalize_resolved_path(&candidate, options));
                        }
                    }
                }
            }

            // Also try the path as-is (it might be a .ts file already)
            if resolution_cache.file_exists(&source_base) {
                return Some(normalize_resolved_path(&source_base, options));
            }
        }
    }

    None
}

pub(crate) fn package_type_from_json(package_json: Option<&PackageJson>) -> Option<PackageType> {
    let package_json = package_json?;

    match package_json.package_type.as_deref() {
        Some("module") => Some(PackageType::Module),
        Some("commonjs") | None => Some(PackageType::CommonJs),
        Some(_) => None,
    }
}

pub(crate) fn read_package_json_uncached(path: &Path) -> Option<PackageJson> {
    // PERF: see `docs/plan/PERFORMANCE_PLAN.md`. Resolver hot path
    // — package.json reads dominate sample profiles on full large-ts-repo.
    count_read_package_json();
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub(crate) fn collect_package_entry_candidates(package_json: &PackageJson) -> Vec<String> {
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
