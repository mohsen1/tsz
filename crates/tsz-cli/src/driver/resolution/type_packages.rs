use rustc_hash::FxHashSet;
use std::path::{Path, PathBuf};

use crate::config::{ModuleResolutionKind, ResolvedCompilerOptions};

#[allow(unused_imports)]
use super::*;

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

            if let Some(entry) =
                resolve_declaration_package_entry(root, candidate, options, None, resolution_cache)
            {
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
    let package_subpath = split_package_specifier(name)
        .and_then(|(package_name, subpath)| subpath.map(|subpath| (package_name, subpath)));
    let conditions = export_conditions(options);

    let mut current = from_file.parent().unwrap_or(base_dir);

    loop {
        let node_modules = current.join("node_modules");
        if resolution_cache.node_modules_dir_exists(&node_modules) {
            if let Some((package_name, subpath)) = package_subpath.as_ref() {
                let package_root = node_modules.join(package_name);
                if resolution_cache.package_root_dir_exists(&package_root) {
                    let package_json =
                        resolution_cache.read_package_json(&package_root.join("package.json"));
                    let resolved = resolve_package_specifier(
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
pub(crate) fn implied_resolution_mode_for_file(file: &Path, base_dir: &Path) -> String {
    let mut cache = ModuleResolutionCache::default();
    implied_resolution_mode_for_file_with_cache(file, base_dir, &mut cache)
}

pub(crate) fn implied_resolution_mode_for_file_with_cache(
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

pub(crate) fn type_package_candidates_for_root(name: &str, root: &Path) -> Vec<String> {
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

pub(crate) fn type_package_candidates(name: &str) -> Vec<String> {
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
            candidates = collect_package_entry_candidates(pj);
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
        let conditions = export_conditions(options);
        let resolved = resolve_package_specifier(
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
    let compiler_version = types_versions_compiler_version(options);
    if let Some(exports) = &package_json.exports
        && let Some(target) = resolve_exports_subpath(exports, ".", &conditions, compiler_version)
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
