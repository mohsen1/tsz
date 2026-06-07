use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

use crate::config::{ModuleResolutionKind, PathMapping, ResolvedCompilerOptions};
use crate::fs::is_valid_module_or_js_file;
use tsz::module_resolver::{PackageType, is_path_relative};

#[allow(unused_imports)]
use super::*;

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
    let from_dir = from_file.parent().unwrap_or(base_dir);
    let package_type = match resolution {
        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
            resolution_cache.package_type_for_dir(from_dir, base_dir)
        }
        _ => None,
    };
    if specifier.starts_with('#') {
        // TypeScript consults tsconfig `paths` before falling back to
        // package.json `imports` for `#`-prefixed specifiers. tsz used to
        // short-circuit straight to `imports`, breaking any project that
        // aliases `#/...` through `paths` (a common convention in
        // Next.js / Vite / modern TypeScript codebases).
        if let Some(resolved) = try_resolve_via_paths(
            &specifier,
            options,
            base_dir,
            resolution_cache,
            known_files,
            package_type,
        ) {
            return Some(resolved);
        }
        if is_invalid_package_import_specifier(&specifier, resolution) {
            return None;
        }
        if options.resolve_package_json_imports {
            return resolve_package_imports_specifier(
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
        extend_path_mapping_candidates(
            &mut candidates,
            &mut path_mapping_attempted,
            &specifier,
            options,
            base_dir,
            resolution_cache,
            package_type,
        );

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
        extend_path_mapping_candidates(
            &mut candidates,
            &mut path_mapping_attempted,
            &specifier,
            options,
            base_dir,
            resolution_cache,
            package_type,
        );

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
        if candidate_exists(&candidate, resolution_cache, known_files) {
            if debug {
                tracing::debug!("candidate={candidate:?} exists=true");
            }
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
                if candidate_exists(&candidate, resolution_cache, known_files) {
                    if debug {
                        tracing::debug!("classic-fallback candidate={candidate:?} exists=true");
                    }
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
        return resolve_node_module_specifier(
            from_file,
            &specifier,
            base_dir,
            options,
            resolution_cache,
        );
    }

    None
}

pub(crate) fn root_dirs_relative_candidates(
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

/// Expand a tsconfig `paths` mapping into candidate paths for `specifier`,
/// without performing an existence check. Returns an empty `Vec` when `paths`
/// is unset, when no mapping matches, or when the mapping has no targets.
///
/// One definition of "what `paths` produces" shared by the classic branch,
/// the node branch, and the `#`-prefix early-out in `resolve_module_specifier`.
fn paths_mapping_candidates(
    specifier: &str,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    resolution_cache: &mut ModuleResolutionCache,
    package_type: Option<PackageType>,
) -> Vec<PathBuf> {
    let Some(paths) = options.paths.as_ref() else {
        return Vec::new();
    };
    let Some((mapping, wildcard)) = resolution_cache.select_path_mapping(paths, specifier) else {
        return Vec::new();
    };
    let base = options.base_url.as_deref().unwrap_or(base_dir);
    let mut candidates = Vec::new();
    for target in &mapping.targets {
        let substituted = substitute_path_target(target, &wildcard);
        let path = if Path::new(&substituted).is_absolute() {
            PathBuf::from(substituted)
        } else {
            base.join(substituted)
        };
        candidates.extend(expand_module_path_candidates(&path, options, package_type));
    }
    candidates
}

/// Append tsconfig-`paths`-derived candidates onto `candidates` and flip
/// `attempted` to `true` if any mapping matched. Shared by the classic and
/// node branches of `resolve_module_specifier`; both used to inline this
/// `let mapped = ...; if !mapped.is_empty() { attempted = true; candidates.extend(mapped); }`
/// block verbatim.
fn extend_path_mapping_candidates(
    candidates: &mut Vec<PathBuf>,
    attempted: &mut bool,
    specifier: &str,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    resolution_cache: &mut ModuleResolutionCache,
    package_type: Option<PackageType>,
) {
    let mapped =
        paths_mapping_candidates(specifier, options, base_dir, resolution_cache, package_type);
    if !mapped.is_empty() {
        *attempted = true;
        candidates.extend(mapped);
    }
}

/// Resolve `specifier` through tsconfig `paths` and return the first existing
/// candidate. The `#`-prefix branch of `resolve_module_specifier` calls this
/// before falling back to package.json `imports`.
fn try_resolve_via_paths(
    specifier: &str,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    resolution_cache: &mut ModuleResolutionCache,
    known_files: &FxHashSet<PathBuf>,
    package_type: Option<PackageType>,
) -> Option<PathBuf> {
    for candidate in
        paths_mapping_candidates(specifier, options, base_dir, resolution_cache, package_type)
    {
        if candidate_exists(&candidate, resolution_cache, known_files) {
            return Some(normalize_resolved_path(&candidate, options));
        }
    }
    None
}

/// Existence predicate for resolver candidates: a path counts as resolvable
/// when it appears in the virtual `known_files` set or is an actual module
/// file on disk. Used by every existence loop in `resolve_module_specifier`.
#[inline]
fn candidate_exists(
    candidate: &Path,
    resolution_cache: &mut ModuleResolutionCache,
    known_files: &FxHashSet<PathBuf>,
) -> bool {
    known_files.contains(candidate) || disk_candidate_exists(candidate, resolution_cache)
}

/// Disk-only existence predicate: the file exists and has a valid module
/// extension. Shared with `package_resolution.rs` sites that don't carry a
/// `known_files` virtual set.
#[inline]
pub(super) fn disk_candidate_exists(
    candidate: &Path,
    resolution_cache: &mut ModuleResolutionCache,
) -> bool {
    resolution_cache.file_exists(candidate) && is_valid_module_or_js_file(candidate)
}

pub(crate) fn select_path_mapping(
    mappings: &[PathMapping],
    specifier: &str,
) -> Option<(usize, String)> {
    // Route through the shared `PathMapping::select_best` so the driver and the
    // `tsz-core` checker resolver pick the same single tsc-best pattern
    // (`matchPatternOrExact` -> `findBestPatternMatch`): an exact wildcard-free
    // key wins outright, otherwise the longest-prefix wildcard. Neither falls
    // through to a less-specific pattern when the chosen one's targets are
    // missing on disk.
    PathMapping::select_best(mappings, specifier)
}

pub(crate) fn substitute_path_target(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replace('*', wildcard)
    } else {
        target.to_string()
    }
}

pub(crate) fn expand_module_path_candidates(
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

pub(crate) fn expand_export_path_candidates(
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

pub(crate) fn split_path_extension(path: &Path) -> Option<(PathBuf, &'static str)> {
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

pub(crate) fn candidates_with_suffixes(path: &Path, suffixes: &[String]) -> Vec<PathBuf> {
    let Some((base, extension)) = split_path_extension(path) else {
        return Vec::new();
    };
    candidates_with_suffixes_and_extension(&base, extension, suffixes)
}

pub(crate) fn candidates_with_suffixes_and_extension(
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

pub(crate) fn path_with_suffix_and_extension(
    base: &Path,
    suffix: &str,
    extension: &str,
) -> Option<PathBuf> {
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

pub(crate) fn node16_extension_substitution(path: &Path, extension: &str) -> Option<Vec<PathBuf>> {
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

pub(crate) const fn extension_candidates_for_resolution(
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
) -> &'static [&'static str] {
    match options.effective_module_resolution() {
        ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => match package_type {
            Some(PackageType::Module) => &NODE16_MODULE_EXTENSION_CANDIDATES,
            Some(PackageType::CommonJs) => &NODE16_COMMONJS_EXTENSION_CANDIDATES,
            None => TS_EXTENSION_CANDIDATES,
        },
        _ => TS_EXTENSION_CANDIDATES,
    }
}

/// Lexically normalize a path: collapse `.`, resolve `..` against the
/// preceding *named* segment, and leave the path otherwise untouched. This is
/// purely textual — it never touches the filesystem — so it is the stable
/// identity key for files that cannot be canonicalized.
///
/// Two corrections over a naive `PathBuf::pop` loop, both of which otherwise
/// let one logical file mint several distinct identity keys:
/// - `..` clamps at the filesystem root / drive prefix (matching `tsc`/Node)
///   instead of popping past it, so an absolute `/a/../../b` stays absolute
///   (`/b`) rather than degrading to a relative `b`.
/// - leading `..` on a relative path is preserved (`../foo` stays `../foo`)
///   instead of being silently dropped.
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    // The collapse algorithm is shared with the `tsz-core` resolver's
    // `normalize_path_segments` via `tsz_common` so the driver's canonical file
    // identity and the resolver's textual identity cannot drift.
    tsz_common::module_resolution::path_identity::normalize_segments(path)
}

pub(crate) fn normalize_resolved_path(path: &Path, options: &ResolvedCompilerOptions) -> PathBuf {
    let normalized = normalize_path(path);
    if options.preserve_symlinks {
        return normalized;
    }
    // When the path cannot be canonicalized (missing or transiently
    // unreadable file, relative anchor), the lexically-normalized path is the
    // only deterministic identity key. Falling back to the *raw* input — as a
    // bare `canonicalize_or_owned` would — lets `./a/b.ts`, `a/b.ts`, and
    // `a/b.ts/` resolve to three distinct IDs for one file, which is precisely
    // the "unstable canonical IDs" symptom. The real-path branch only matters
    // when canonicalization actually succeeds.
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return normalized;
    };
    let preserve_package_link_identity = path_has_symlinked_package_ancestor(path)
        || (!has_node_modules_component(path) && has_node_modules_component(&canonical));
    if preserve_package_link_identity {
        normalized
    } else {
        canonical
    }
}

/// Find the innermost `node_modules/<package>/` root for a file path.
pub(crate) fn find_node_modules_package_root(path: &Path) -> Option<PathBuf> {
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

pub(crate) fn path_has_symlinked_package_ancestor(path: &Path) -> bool {
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

pub(crate) fn has_node_modules_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            std::path::Component::Normal(part) if part.to_str() == Some("node_modules")
        )
    })
}

/// Directory from which a `node_modules` walk-up should begin for resolutions
/// originating in `from_file`.
///
/// `tsc` performs module resolution relative to the *real* on-disk location of
/// a file (`preserveSymlinks: false`, the default). When a package is installed
/// through a symlink — pnpm hoists `node_modules/<pkg>` to a symlink whose
/// target lives in an isolated `.pnpm/<pkg>@<version>/node_modules` sandbox —
/// that sandbox holds the package's private (transitive) dependencies. Walking
/// up from the *symlink* path only reaches the top-level `node_modules`, so
/// transitive `@types/*` siblings referenced via `/// <reference types="..." />`
/// or bare `import`/`require` from inside the package are missed, producing
/// spurious `TS2688`/`TS2307`.
///
/// Resolving the real path of the containing directory restores parity: the
/// walk-up then traverses the sandbox and finds the siblings. The file's
/// program identity (its symlink-relative display path) is untouched — only the
/// lookup anchor changes, mirroring how `tsc` separates module identity from
/// the realpath used for resolution.
///
/// The probe is gated so ordinary project files never pay a `realpath` syscall:
/// `preserveSymlinks` disables it (matching `tsc`), and the `realpath` is only
/// taken when the file actually lives inside a symlinked `node_modules` package.
///
/// Use this only for *cross-package* walk-ups — resolving a different package
/// (a bare specifier or a `/// <reference types>` sibling). Walk-ups that stay
/// *inside* the containing package (package.json `imports`, self-reference,
/// nearest-`package.json` mode detection) must keep the symlink-relative anchor
/// so intra-package files retain their symlink identity (see
/// `normalize_resolved_path`); those paths are fully reachable through the
/// symlink and have no sandbox blind spot.
pub(crate) fn node_modules_walkup_dir(
    from_file: &Path,
    base_dir: &Path,
    options: &ResolvedCompilerOptions,
) -> PathBuf {
    let dir = from_file.parent().unwrap_or(base_dir);
    // The cheap `node_modules` path scan short-circuits the per-ancestor
    // symlink probe for ordinary project files.
    let needs_realpath = !options.preserve_symlinks
        && has_node_modules_component(from_file)
        && path_has_symlinked_package_ancestor(from_file);
    if needs_realpath {
        canonicalize_or_owned(dir)
    } else {
        dir.to_path_buf()
    }
}

pub(crate) fn is_root_alias_symlink(dir: &Path) -> bool {
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
/// `node_modules` paths). Every non-canonical copy redirects directly to the
/// rank-winning canonical copy.
///
/// Rank is `(node_modules-depth, normalized-path)`: the shallowest copy wins,
/// with lexical path used as a deterministic tie-break. The function sorts the
/// discovered package roots by rank first, then sweeps once — within each
/// contiguous `(name, version)` run the first entry is the canonical winner
/// and every later entry redirects straight to it. The sort makes this a
/// pure function of the input file set (no `FxHashSet` iteration order leaks
/// in) and the rank-ordered sweep skips the stale-chain hazard an
/// "encounter order" loop has to fix up after the fact.
pub(crate) fn build_duplicate_package_redirects(
    file_names: &[String],
    options: &ResolvedCompilerOptions,
) -> FxHashMap<PathBuf, PathBuf> {
    // Map each input file to its `node_modules` package root once. The
    // file-redirect pass below reuses these without re-walking the path.
    let mut file_pkg_roots: Vec<Option<PathBuf>> = Vec::with_capacity(file_names.len());
    let mut package_roots: FxHashSet<PathBuf> = FxHashSet::default();
    for file_name in file_names {
        let pkg_root = find_node_modules_package_root(Path::new(file_name));
        if let Some(ref root) = pkg_root {
            package_roots.insert(root.clone());
        }
        file_pkg_roots.push(pkg_root);
    }

    for root in &package_roots {
        tracing::debug!(target: "tsz::dup_pkg", root = %root.display(), "found package root");
    }

    // Pre-resolve each package root's `(name, version)` identity. The sort
    // key adds `(node_modules-depth, normalized-path)` as the rank tie-break,
    // computed once per root via `sort_by_cached_key` instead of on every
    // comparator call.
    let mut root_identities: Vec<(PathBuf, String, String)> = Vec::new();
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
        root_identities.push((pkg_root.clone(), name, version));
    }
    root_identities.sort_by_cached_key(|(pkg_root, name, version)| {
        let depth = pkg_root
            .components()
            .filter(|c| c.as_os_str() == "node_modules")
            .count();
        (
            name.clone(),
            version.clone(),
            depth,
            normalize_resolved_path(pkg_root, options)
                .to_string_lossy()
                .into_owned(),
        )
    });

    // Within each `(name, version)` run the first element is canonical and
    // every later element redirects directly to it. `chunk_by` makes the run
    // structure explicit and removes the manual `last_key`/`canonical_root`
    // trackers a fold-style sweep would need.
    let mut root_redirects: FxHashMap<PathBuf, PathBuf> = FxHashMap::default();
    for run in root_identities.chunk_by(|left, right| left.1 == right.1 && left.2 == right.2) {
        let [(canon, ..), rest @ ..] = run else {
            continue;
        };
        for (pkg_root, _, _) in rest {
            if pkg_root == canon {
                continue;
            }
            root_redirects.insert(pkg_root.clone(), canon.clone());
        }
    }
    for (from, to) in &root_redirects {
        tracing::debug!(target: "tsz::dup_pkg", from = %from.display(), to = %to.display(), "root redirect");
    }
    // Cache `normalize_resolved_path(canonical_root)` per redirected root.
    // `normalize_resolved_path` calls `canonicalize` + walks every ancestor
    // looking for symlinks, so the cache turns N files-under-one-package into
    // one canonicalize per package instead of one per file.
    let mut normalized_canonical: FxHashMap<&Path, PathBuf> = FxHashMap::default();
    let mut file_redirects: FxHashMap<PathBuf, PathBuf> = FxHashMap::default();
    for (file_name, pkg_root) in file_names.iter().zip(file_pkg_roots.iter()) {
        let Some(pkg_root) = pkg_root else { continue };
        let Some(canonical_root) = root_redirects.get(pkg_root) else {
            continue;
        };
        let file_path = Path::new(file_name);
        let Ok(relative) = file_path.strip_prefix(pkg_root) else {
            continue;
        };
        let canonical_root_normalized = normalized_canonical
            .entry(canonical_root.as_path())
            .or_insert_with(|| normalize_resolved_path(canonical_root, options));
        let to = canonical_root_normalized.join(relative);
        let from = normalize_resolved_path(file_path, options);
        tracing::debug!(target: "tsz::dup_pkg", from = %from.display(), to = %to.display(), "file redirect");
        if from != to {
            file_redirects.insert(from, to);
        }
    }
    file_redirects
}

pub(crate) const KNOWN_EXTENSIONS: [&str; 12] = [
    ".d.mts", ".d.cts", ".d.ts", ".mts", ".cts", ".tsx", ".ts", ".mjs", ".cjs", ".jsx", ".js",
    ".json",
];
/// TS-only candidate priority for non-Node16 fan-outs (path mapping, baseUrl,
/// classic, bundler). Mirrors tsc's `supportedTSExtensions` via the shared
/// constant in `tsz-common::file_extensions` so all crates use the same order.
pub(crate) const TS_EXTENSION_CANDIDATES: &[&str] =
    tsz_common::file_extensions::TSC_TS_RESOLUTION_EXTENSIONS_BARE;
pub(crate) const PACKAGE_INDEX_FALLBACK_EXTENSIONS: [&str; 3] = ["ts", "tsx", "d.ts"];
pub(crate) const PACKAGE_INDEX_FALLBACK_ALLOW_JS_EXTENSIONS: [&str; 5] =
    ["ts", "tsx", "d.ts", "js", "jsx"];

pub(crate) const NODE16_MODULE_EXTENSION_CANDIDATES: [&str; 7] =
    ["mts", "d.mts", "ts", "tsx", "d.ts", "cts", "d.cts"];
pub(crate) const NODE16_COMMONJS_EXTENSION_CANDIDATES: [&str; 7] =
    ["cts", "d.cts", "ts", "tsx", "d.ts", "mts", "d.mts"];

#[cfg(test)]
#[path = "paths_imports_tests.rs"]
mod paths_imports_tests;

#[cfg(test)]
#[path = "canonical_id_tests.rs"]
mod canonical_id_tests;
