//! Inheritance / `extends` handling for tsconfig.
//!
//! This submodule owns one of the option domains carved out of the historic
//! `config/mod.rs` monolith (see issue #8280): resolving `extends` paths
//! (relative, absolute, `node_modules` package, and `package.json#exports`),
//! anchoring inherited path-shaped options at the *declaring* config's
//! directory, and merging two `TsConfig`/`CompilerOptions` values where the
//! child overrides the base.
//!
//! The functions here are behavior-preserving moves from `mod.rs`. They
//! intentionally remain `pub(super)` rather than `pub` so the public
//! `config` API is still gated through `mod.rs`.
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use crate::module_resolver_helpers::{
    PackageExports, PackageJson, find_best_export_pattern, match_export_pattern,
    parse_package_specifier, substitute_wildcard_in_exports,
};

use super::{CompilerOptions, TsConfig};
use tsz_common::file_extensions::{JSON_EXTENSION, is_json_file};
use tsz_common::module_resolution::path_identity::normalize_segments;

/// The outcome of resolving a tsconfig `extends` specifier, carrying the
/// distinction `tsc` draws between an unresolvable specifier (TS6053) and a
/// resolved-but-unreadable file (TS5083).
///
/// `tsc`'s `getExtendsConfigPath` splits three ways, which this mirrors:
/// - a specifier that names an existing config file → [`Self::Found`];
/// - a **rooted/relative** specifier that already carries a `.json` extension
///   but whose file does not exist → [`Self::Unreadable`]. `tsc` returns that
///   path from `getExtendsConfigPath` *unchecked* (the `.json`-append fallback
///   only fires for extensionless specifiers), so the subsequent file read
///   fails and surfaces a file-less **TS5083** anchored at the resolved path;
/// - any other miss — an extensionless/non-`.json` relative specifier whose
///   `.json`-appended candidate is also absent, or a bare/package specifier
///   that fails Node resolution → [`Self::NotFound`], the specifier-anchored
///   **TS6053**.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ExtendsResolution {
    /// An existing config file to load and merge.
    Found(PathBuf),
    /// A rooted/relative `.json` specifier that resolved to a concrete path
    /// which does not exist; the payload is the lexically normalized absolute
    /// path `tsc` embeds in TS5083's `Cannot read file '{0}'.`
    Unreadable(PathBuf),
    /// The specifier never resolved to any candidate file; the caller reports
    /// TS6053 anchored at the specifier literal.
    NotFound,
}

/// Resolve a tsconfig `extends` specifier to the config file it names on disk.
///
/// Returns an [`ExtendsResolution`] describing whether the specifier located an
/// existing file, resolved to an unreadable `.json` path (TS5083), or failed to
/// resolve at all (TS6053) — and `Err` only for a malformed `current_path`.
///
/// Resolution mirrors `tsc`'s `getExtendsConfigPath`:
/// - **Relative / absolute** specifiers (`./base`, `../base.json`, `/abs`)
///   resolve against the declaring config's directory. The path is lexically
///   normalized, probed as written, and — only when it carries no `.json`
///   extension — re-probed with `.json` appended. A missing `.json` specifier
///   is [`ExtendsResolution::Unreadable`]; a missing extensionless one is
///   [`ExtendsResolution::NotFound`]. No directory lookup is attempted (a
///   relative `extends` must name a file, like `tsc`).
/// - **Non-relative** specifiers go through Node module resolution: first the
///   package's `package.json` `"exports"` map, then a `node_modules` walk that
///   honors an explicit file subpath (`pkg/base.json`), an extensionless
///   subpath (`pkg/recommended` -> `recommended.json`), and a bare package
///   whose root holds a `tsconfig.json`. Every module-resolution miss is
///   [`ExtendsResolution::NotFound`] (TS6053) — `tsc` never reports TS5083 for
///   a package specifier.
pub(super) fn resolve_extends_path(
    current_path: &Path,
    extends: &str,
) -> Result<ExtendsResolution> {
    let base_dir = current_path
        .parent()
        .ok_or_else(|| anyhow!("tsconfig has no parent directory"))?;

    // Relative or absolute path: resolve against the declaring config's dir.
    if extends.starts_with('.') || extends.starts_with('/') {
        let joined = if Path::new(extends).is_absolute() {
            PathBuf::from(extends)
        } else {
            base_dir.join(extends)
        };
        // `tsc` normalizes the specifier against the config directory before
        // probing, so the path it feeds to the file read (and thus TS5083's
        // message) is lexically normalized — `../base.json` becomes an absolute
        // sibling path, not a `<dir>/../base.json` spelling.
        let candidate = normalize_segments(&joined);
        if candidate.is_file() {
            return Ok(ExtendsResolution::Found(candidate));
        }
        // A specifier that already ends in `.json` is returned by `tsc`
        // unchecked; the ensuing file read fails with TS5083 at the resolved
        // path rather than the generic specifier-anchored TS6053.
        if is_json_file(&candidate) {
            return Ok(ExtendsResolution::Unreadable(candidate));
        }
        // Otherwise `tsc` appends `.json` and re-probes; a miss there is the
        // specifier-anchored TS6053.
        let with_json = append_json_extension(candidate);
        if with_json.is_file() {
            return Ok(ExtendsResolution::Found(with_json));
        }
        return Ok(ExtendsResolution::NotFound);
    }

    // Non-relative: Node module resolution. `package.json` exports first.
    if let Some(resolved) = resolve_package_extends_path(current_path, extends) {
        return Ok(ExtendsResolution::Found(resolved));
    }

    // Package-name extends (e.g. "@tsconfig/node20/tsconfig.json").
    // Walk `node_modules` upward through directory ancestors.
    let mut search_dir = base_dir.to_path_buf();
    loop {
        let candidate = search_dir.join("node_modules").join(extends);
        if let Some(resolved) = probe_extends_candidate(&candidate, true) {
            return Ok(ExtendsResolution::Found(resolved));
        }
        if !search_dir.pop() {
            break;
        }
    }

    Ok(ExtendsResolution::NotFound)
}

/// Append a literal `.json` to a path, matching `tsc`'s `path + Extension.Json`
/// (a suffix append, never an extension *replacement*): `foo.bar` becomes
/// `foo.bar.json`, not `foo.json` (which is what `Path::with_extension` would
/// produce). Consumes `path` so the caller's dead `PathBuf` is reused.
fn append_json_extension(path: PathBuf) -> PathBuf {
    let mut os = path.into_os_string();
    os.push(JSON_EXTENSION);
    PathBuf::from(os)
}

/// Probe a single candidate path for an `extends` target, returning the
/// existing config file it names or `None`.
///
/// Tries, in order: the candidate as written, the candidate with a `.json`
/// extension appended when it has none, and — only when `allow_dir_tsconfig`
/// is set (the `node_modules` package-root case) — a `tsconfig.json` inside the
/// candidate directory. Existence is checked at every step so a miss is
/// reported as `None` rather than a path that does not exist.
fn probe_extends_candidate(candidate: &Path, allow_dir_tsconfig: bool) -> Option<PathBuf> {
    if candidate.is_file() {
        return Some(candidate.to_path_buf());
    }
    if candidate.extension().is_none() {
        let with_json = candidate.with_extension("json");
        if with_json.is_file() {
            return Some(with_json);
        }
    }
    if allow_dir_tsconfig && candidate.is_dir() {
        let nested = candidate.join("tsconfig.json");
        if nested.is_file() {
            return Some(nested);
        }
    }
    None
}

#[cfg(target_arch = "wasm32")]
fn resolve_package_extends_path(_current_path: &Path, _extends: &str) -> Option<PathBuf> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_package_extends_path(current_path: &Path, extends: &str) -> Option<PathBuf> {
    let base_dir = current_path.parent()?;
    let (package_name, subpath) = parse_package_specifier(extends);
    let export_subpath = subpath
        .as_deref()
        .map(|value| format!("./{value}"))
        .unwrap_or_else(|| ".".to_string());

    let mut search_dir = base_dir.to_path_buf();
    loop {
        let package_dir = search_dir.join("node_modules").join(&package_name);
        let package_json_path = package_dir.join("package.json");
        if package_json_path.is_file()
            && let Some(package_json) = read_package_json_for_extends(&package_json_path)
            && let Some(exports) = &package_json.exports
            && let Some(resolved) =
                resolve_package_extends_exports(&package_dir, exports, &export_subpath)
        {
            return Some(resolved);
        }

        if !search_dir.pop() {
            break;
        }
    }

    None
}

#[cfg(not(target_arch = "wasm32"))]
fn read_package_json_for_extends(path: &Path) -> Option<PackageJson> {
    let source = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&source).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_package_extends_exports(
    package_dir: &Path,
    exports: &PackageExports,
    subpath: &str,
) -> Option<PathBuf> {
    const CONDITIONS: &[&str] = &["types", "node", "import", "require", "default"];

    match exports {
        PackageExports::String(target) => {
            if subpath == "." {
                resolve_config_export_target(package_dir, target)
            } else {
                None
            }
        }
        PackageExports::Map(map) => {
            if let Some(value) = map.get(subpath) {
                return resolve_package_extends_export_value(package_dir, value, CONDITIONS);
            }

            if let Some((pattern, wildcard, value)) =
                find_best_export_pattern(map.iter(), |p| match_export_pattern(p, subpath))
            {
                // Directory-match keys end in `/` and have no `*`; only
                // those should append the wildcard to a `/`-ending target.
                let is_directory_match = pattern.ends_with('/') && !pattern.contains('*');
                let substituted_value =
                    substitute_wildcard_in_exports(value, &wildcard, is_directory_match);
                return resolve_package_extends_export_value(
                    package_dir,
                    &substituted_value,
                    CONDITIONS,
                );
            }

            None
        }
        PackageExports::Conditional(entries) => {
            for (key, value) in entries {
                if CONDITIONS.iter().any(|condition| condition == key) {
                    if matches!(value, PackageExports::Null) {
                        return None;
                    }
                    if let Some(resolved) =
                        resolve_package_extends_exports(package_dir, value, subpath)
                    {
                        return Some(resolved);
                    }
                }
            }
            None
        }
        PackageExports::Array(elements) => {
            for element in elements {
                if let Some(resolved) =
                    resolve_package_extends_exports(package_dir, element, subpath)
                {
                    return Some(resolved);
                }
            }
            None
        }
        PackageExports::Null => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_package_extends_export_value(
    package_dir: &Path,
    value: &PackageExports,
    conditions: &[&str],
) -> Option<PathBuf> {
    match value {
        PackageExports::String(target) => resolve_config_export_target(package_dir, target),
        PackageExports::Conditional(entries) => {
            for (key, nested) in entries {
                if conditions.iter().any(|condition| condition == key) {
                    if matches!(nested, PackageExports::Null) {
                        return None;
                    }
                    if let Some(resolved) =
                        resolve_package_extends_export_value(package_dir, nested, conditions)
                    {
                        return Some(resolved);
                    }
                }
            }
            None
        }
        PackageExports::Array(elements) => {
            for element in elements {
                if let Some(resolved) =
                    resolve_package_extends_export_value(package_dir, element, conditions)
                {
                    return Some(resolved);
                }
            }
            None
        }
        PackageExports::Map(_) | PackageExports::Null => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_config_export_target(package_dir: &Path, target: &str) -> Option<PathBuf> {
    // A package `"exports"` target names a package-relative path; probe it the
    // same way as a `node_modules` extends candidate (exact file, `.json`
    // append, or directory `tsconfig.json`).
    let resolved = package_dir.join(target.trim_start_matches("./"));
    probe_extends_candidate(&resolved, true)
}

/// Anchor relative path-like compiler options at the directory of the
/// tsconfig that declared them. `tsc` resolves `baseUrl` relative to the
/// config file where it is written, so when one config inherits from
/// another via `extends` the inherited path must stay anchored at the
/// *base* config's directory rather than the consuming child's. We
/// perform that anchoring at load time so the merged `CompilerOptions`
/// carries an absolute path that downstream CLI normalizers leave alone.
pub(super) fn anchor_inherited_path_options(config: &mut TsConfig, config_path: &Path) {
    let Some(parent) = config_path.parent() else {
        return;
    };
    let Some(opts) = config.compiler_options.as_mut() else {
        return;
    };
    anchor_relative_path_option(&mut opts.base_url, parent);
    anchor_relative_path_option(&mut opts.root_dir, parent);
    anchor_relative_path_option(&mut opts.out_dir, parent);
    anchor_relative_path_option(&mut opts.declaration_dir, parent);
    anchor_relative_path_option(&mut opts.ts_build_info_file, parent);

    if let Some(root_dirs) = opts.root_dirs.as_mut() {
        let parent_abs = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        for root_dir in root_dirs {
            let trimmed = root_dir.trim();
            if trimmed.is_empty() {
                continue;
            }
            let candidate = std::path::Path::new(trimmed);
            if candidate.is_absolute() {
                continue;
            }
            let joined = parent_abs.join(candidate);
            let normalized = std::fs::canonicalize(&joined).unwrap_or(joined);
            *root_dir = normalized.to_string_lossy().into_owned();
        }
    }

    if let Some(type_roots) = opts.type_roots.as_mut() {
        let parent_abs = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        for type_root in type_roots {
            let trimmed = type_root.trim();
            if trimmed.is_empty() {
                continue;
            }
            let candidate = std::path::Path::new(trimmed);
            if candidate.is_absolute() {
                continue;
            }
            let joined = parent_abs.join(candidate);
            let normalized = std::fs::canonicalize(&joined).unwrap_or(joined);
            *type_root = normalized.to_string_lossy().into_owned();
        }
    }
}

fn anchor_relative_path_option(option: &mut Option<String>, base_dir: &Path) {
    let Some(value) = option.as_deref() else {
        return;
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let candidate = std::path::Path::new(trimmed);
    if candidate.is_absolute() {
        return;
    }

    let base_abs = std::fs::canonicalize(base_dir).unwrap_or_else(|_| base_dir.to_path_buf());
    let joined = base_abs.join(candidate);
    let normalized = std::fs::canonicalize(&joined).unwrap_or(joined);
    *option = Some(normalized.to_string_lossy().into_owned());
}

/// The TypeScript 5.5 `${configDir}` tsconfig template variable.
///
/// When a path-shaped tsconfig field begins with this token, `tsc` substitutes
/// it with the directory of the config file that is being compiled in this
/// invocation — i.e. for an `extends` chain it resolves to the *inheriting*
/// (leaf) config's directory, never the base config's own directory. That is
/// the whole point of the feature: a shared base config can write
/// `"${configDir}/src"` and have every consumer resolve it against the
/// consumer's directory.
const CONFIG_DIR_TEMPLATE: &str = "${configDir}";

/// Substitute the `${configDir}` template in every path-shaped tsconfig field
/// against `config_dir`, the directory of the root config being compiled.
///
/// `config_dir` is the same value for every config in an `extends` chain (the
/// leaf/inheriting config's directory), so callers thread the root directory
/// through the recursion unchanged. Substitution must run *before* the
/// `extends` anchoring helpers: it rewrites a leading `${configDir}` into an
/// absolute, lexically-normalized path, which the `anchor_*` helpers then leave
/// untouched (they skip already-absolute values). Fields whose value does not
/// start with the template are left exactly as written so ordinary relative
/// paths keep being anchored at their declaring config's directory.
pub(super) fn substitute_config_dir_templates(config: &mut TsConfig, config_dir: &Path) {
    // Root file selectors may carry glob metacharacters (`**`, `*.ts`), so they
    // are normalized lexically (never canonicalized) just like the anchoring
    // path for inherited selectors.
    for selectors in [
        config.files.as_mut(),
        config.include.as_mut(),
        config.exclude.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        for selector in selectors {
            substitute_config_dir_in_place(selector, config_dir);
        }
    }

    let Some(opts) = config.compiler_options.as_mut() else {
        return;
    };
    for option in [
        &mut opts.base_url,
        &mut opts.root_dir,
        &mut opts.out_dir,
        &mut opts.declaration_dir,
        &mut opts.out_file,
        &mut opts.ts_build_info_file,
    ] {
        if let Some(value) = option.as_mut() {
            substitute_config_dir_in_place(value, config_dir);
        }
    }
    for list in [opts.root_dirs.as_mut(), opts.type_roots.as_mut()]
        .into_iter()
        .flatten()
    {
        for entry in list {
            substitute_config_dir_in_place(entry, config_dir);
        }
    }
    if let Some(paths) = opts.paths.as_mut() {
        for substitutions in paths.values_mut() {
            for substitution in substitutions {
                substitute_config_dir_in_place(substitution, config_dir);
            }
        }
    }
}

/// Rewrite a single string in place when it begins with `${configDir}`.
fn substitute_config_dir_in_place(value: &mut String, config_dir: &Path) {
    if let Some(replaced) = substitute_config_dir(value, config_dir) {
        *value = replaced;
    }
}

/// Replace a leading `${configDir}` token with `config_dir` and lexically
/// normalize the result. Returns `None` when the value does not start with the
/// template, so non-template paths are left byte-for-byte unchanged.
///
/// Mirrors `tsc`'s `getSubstitutedPathWithConfigDirTemplate`, which rewrites the
/// template to `./` and resolves it against the config directory: a bare
/// `${configDir}` becomes the directory itself, and `${configDir}/src` becomes
/// `<config_dir>/src`. The template is honored only at the start of the value
/// (the TS spec restricts it to the leading segment).
fn substitute_config_dir(value: &str, config_dir: &Path) -> Option<String> {
    let rest = value.strip_prefix(CONFIG_DIR_TEMPLATE)?;
    // Drop the single separator that follows the token so the remainder joins
    // as a relative path; `${configDir}` on its own resolves to the directory.
    let rest = rest
        .strip_prefix('/')
        .or_else(|| rest.strip_prefix('\\'))
        .unwrap_or(rest);
    let joined = if rest.is_empty() {
        config_dir.to_path_buf()
    } else {
        config_dir.join(rest)
    };
    Some(
        lexically_normalize_selector(&joined)
            .to_string_lossy()
            .into_owned(),
    )
}

pub(super) fn anchor_inherited_root_selectors(config: &mut TsConfig, config_path: &Path) {
    let Some(parent) = config_path.parent() else {
        return;
    };
    let parent_abs = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());

    if let Some(files) = config.files.as_mut() {
        for file in files {
            anchor_relative_selector(file, &parent_abs);
        }
    }
    if let Some(include) = config.include.as_mut() {
        for pattern in include {
            anchor_relative_selector(pattern, &parent_abs);
        }
    }
    if let Some(exclude) = config.exclude.as_mut() {
        for pattern in exclude {
            anchor_relative_selector(pattern, &parent_abs);
        }
    }
}

fn anchor_relative_selector(selector: &mut String, base_dir: &Path) {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return;
    }
    let candidate = std::path::Path::new(trimmed);
    if candidate.is_absolute() {
        return;
    }
    let joined = base_dir.join(candidate);
    *selector = lexically_normalize_selector(&joined)
        .to_string_lossy()
        .into_owned();
}

/// Lexically collapse `.` and `..` components in an anchored selector path
/// without touching the filesystem.
///
/// Anchoring a relative `include`/`exclude`/`files` selector onto its
/// declaring config's directory via [`Path::join`] preserves any leading
/// `./` (or embedded `..`) the selector carried — e.g. a base config's
/// `"./global.d.ts"` becomes `<dir>/./global.d.ts`. That is a valid
/// filesystem path but an *unmatchable glob*: during glob matching the `.`
/// is a literal path component, so the pattern never matches the real
/// `<dir>/global.d.ts` and discovery reports zero inputs (a false `TS18003`).
///
/// This rebuilds the path from [`Path::components`], which preserves glob
/// metacharacters such as `**`/`*.ts` (unlike [`std::fs::canonicalize`], which
/// hits the filesystem) and removes the `.`/`..` segments. The sibling
/// `resolution::helpers::normalize_path_segments` is deliberately *not* reused
/// here: it short-circuits to the original string when `components()` surfaces
/// no `.`/`..`, but `components()` silently drops *embedded* `.` segments, so a
/// joined `<dir>/./glob` would be returned with its `/./` intact.
///
/// The canonical `path_identity::normalize_segments` is also deliberately
/// *not* reused: it clamps a `..` at the filesystem root (the `tsc`/Node
/// module-identity rule), while a glob selector must keep an unmatched `..`
/// so an anchored pattern never silently changes which directory it matches.
fn lexically_normalize_selector(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Resolve `..` against a preceding concrete component only.
                // Never pop a root/prefix, and keep `..` when there is nothing
                // ordinary to cancel (it would otherwise change the meaning).
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                } else {
                    normalized.push("..");
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

pub(super) fn merge_configs(base: TsConfig, mut child: TsConfig) -> TsConfig {
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
        // references are not inherited from extended configs (tsc behavior)
        references: child.references,
    }
}

/// Merge two `CompilerOptions` structs, preferring child values over base.
/// Every `Option` field in `CompilerOptions` uses `.or()` — child wins when present.
macro_rules! merge_options {
    ($child:expr, $base:expr, $Struct:ident { $($field:ident),* $(,)? }) => {
        $Struct { $( $field: $child.$field.or($base.$field), )* ..Default::default() }
    };
}

fn merge_compiler_options(base: CompilerOptions, child: CompilerOptions) -> CompilerOptions {
    // Merge invalidated_options from both base and child (child takes priority).
    let mut invalidated = child.invalidated_options.clone();
    invalidated.extend(base.invalidated_options.iter().cloned());
    let mut merged = merge_options!(
        child,
        base,
        CompilerOptions {
            target,
            module,
            module_resolution,
            resolve_package_json_exports,
            resolve_package_json_imports,
            module_suffixes,
            resolve_json_module,
            allow_arbitrary_extensions,
            allow_importing_ts_extensions,
            rewrite_relative_import_extensions,
            types_versions_compiler_version,
            types,
            type_roots,
            jsx,
            jsx_factory,
            jsx_fragment_factory,
            jsx_import_source,
            react_namespace,

            lib,
            no_lib,
            lib_replacement,
            no_types_and_symbols,
            base_url,
            paths,
            root_dir,
            root_dirs,
            out_dir,
            out_file,
            composite,
            declaration,
            emit_declaration_only,
            declaration_dir,
            source_map,
            inline_source_map,
            declaration_map,
            ts_build_info_file,
            incremental,
            strict,
            sound,
            no_emit,
            emit_bom,
            no_check,
            preserve_symlinks,
            no_emit_on_error,
            isolated_modules,
            isolated_declarations,
            verbatim_module_syntax,
            custom_conditions,
            es_module_interop,
            allow_synthetic_default_imports,
            experimental_decorators,
            emit_decorator_metadata,
            import_helpers,
            no_emit_helpers,
            downlevel_iteration,
            remove_comments,
            new_line,
            allow_js,
            check_js,
            skip_lib_check,
            skip_default_lib_check,
            strip_internal,
            always_strict,
            use_define_for_class_fields,
            no_implicit_any,
            no_implicit_returns,
            strict_null_checks,
            strict_function_types,
            strict_property_initialization,
            no_implicit_this,
            use_unknown_in_catch_variables,
            strict_bind_call_apply,
            strict_builtin_iterator_return,
            exact_optional_property_types,
            no_unchecked_indexed_access,
            no_property_access_from_index_signature,
            no_unused_locals,
            no_unused_parameters,
            allow_unreachable_code,
            allow_unused_labels,
            no_fallthrough_cases_in_switch,
            no_resolve,
            no_unchecked_side_effect_imports,
            no_implicit_override,
            module_detection,
            ignore_deprecations,
            allow_umd_global_access,
            preserve_const_enums,
            erasable_syntax_only,
            max_node_module_js_depth,
        }
    );
    merged.invalidated_options = invalidated;
    merged
}

#[cfg(test)]
mod tests {
    use super::super::TsConfigReference;
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn substitute_config_dir_expands_root_selectors_and_path_options() {
        let config_dir = Path::new("/proj/app");
        let mut config = TsConfig {
            include: Some(vec![
                "${configDir}/src".to_string(),
                "src/**/*.ts".to_string(),
            ]),
            exclude: Some(vec!["${configDir}/dist".to_string()]),
            files: Some(vec!["${configDir}/entry.ts".to_string()]),
            compiler_options: Some(CompilerOptions {
                base_url: Some("${configDir}".to_string()),
                out_dir: Some("${configDir}/dist".to_string()),
                type_roots: Some(vec![
                    "${configDir}/types".to_string(),
                    "./node_modules/@types".to_string(),
                ]),
                paths: Some(
                    [("@app/*".to_string(), vec!["${configDir}/src/*".to_string()])]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        };

        substitute_config_dir_templates(&mut config, config_dir);

        let include = config.include.as_ref().unwrap();
        assert_eq!(
            include[0], "/proj/app/src",
            "${{configDir}}/src resolves against the root config dir"
        );
        assert_eq!(
            include[1], "src/**/*.ts",
            "non-template selectors are left for the extends anchoring step"
        );
        assert_eq!(config.exclude.as_ref().unwrap()[0], "/proj/app/dist");
        assert_eq!(config.files.as_ref().unwrap()[0], "/proj/app/entry.ts");

        let opts = config.compiler_options.as_ref().unwrap();
        assert_eq!(
            opts.base_url.as_deref(),
            Some("/proj/app"),
            "bare ${{configDir}} resolves to the directory itself"
        );
        assert_eq!(opts.out_dir.as_deref(), Some("/proj/app/dist"));
        let type_roots = opts.type_roots.as_ref().unwrap();
        assert_eq!(type_roots[0], "/proj/app/types");
        assert_eq!(
            type_roots[1], "./node_modules/@types",
            "non-template entries untouched"
        );
        assert_eq!(opts.paths.as_ref().unwrap()["@app/*"][0], "/proj/app/src/*");
    }

    #[test]
    fn substitute_config_dir_only_matches_leading_token() {
        let config_dir = Path::new("/proj");
        let mut config = TsConfig {
            // The TS spec only honors `${configDir}` at the start of a value.
            include: Some(vec!["src/${configDir}/x".to_string()]),
            ..Default::default()
        };
        substitute_config_dir_templates(&mut config, config_dir);
        assert_eq!(
            config.include.as_ref().unwrap()[0],
            "src/${configDir}/x",
            "a non-leading template is left literal, matching tsc"
        );
    }

    #[test]
    fn merge_configs_child_overrides_base_compiler_options() {
        let base = TsConfig {
            compiler_options: Some(CompilerOptions {
                strict: Some(false),
                target: Some("ES5".to_string()),
                ..Default::default()
            }),
            include: Some(vec!["base/**/*".to_string()]),
            ..Default::default()
        };
        let child = TsConfig {
            compiler_options: Some(CompilerOptions {
                strict: Some(true),
                ..Default::default()
            }),
            include: Some(vec!["child/**/*".to_string()]),
            ..Default::default()
        };

        let merged = merge_configs(base, child);

        let opts = merged.compiler_options.expect("merged compiler options");
        assert_eq!(opts.strict, Some(true), "child overrides base");
        assert_eq!(
            opts.target.as_deref(),
            Some("ES5"),
            "child does not erase base when unset"
        );
        assert_eq!(
            merged.include.as_deref(),
            Some(&["child/**/*".to_string()][..]),
            "child include wins"
        );
    }

    #[test]
    fn merge_configs_child_compiler_options_absent_keeps_base() {
        let base = TsConfig {
            compiler_options: Some(CompilerOptions {
                strict: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let child = TsConfig::default();

        let merged = merge_configs(base, child);
        assert_eq!(
            merged.compiler_options.as_ref().and_then(|o| o.strict),
            Some(true)
        );
    }

    #[test]
    fn merge_compiler_options_invalidated_combines_child_first() {
        let base = CompilerOptions {
            invalidated_options: vec!["target".to_string()],
            ..Default::default()
        };
        let child = CompilerOptions {
            invalidated_options: vec!["module".to_string()],
            ..Default::default()
        };

        let merged = merge_compiler_options(base, child);
        assert_eq!(
            merged.invalidated_options,
            vec!["module".to_string(), "target".to_string()],
            "child invalidations come first, then base"
        );
    }

    #[test]
    fn merge_configs_references_only_from_child() {
        let base = TsConfig {
            references: Some(vec![TsConfigReference {
                path: "../base-ref".to_string(),
                prepend: false,
            }]),
            ..Default::default()
        };
        let child = TsConfig::default();

        let merged = merge_configs(base, child);
        assert!(
            merged.references.is_none(),
            "references must not inherit through extends"
        );
    }

    #[test]
    fn anchor_inherited_root_selectors_makes_relative_paths_absolute() {
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("nested");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("tsconfig.json");

        let mut config = TsConfig {
            include: Some(vec!["src/**/*".to_string(), "/already/abs".to_string()]),
            exclude: Some(vec!["node_modules".to_string()]),
            files: Some(vec!["entry.ts".to_string()]),
            ..Default::default()
        };

        anchor_inherited_root_selectors(&mut config, &config_path);
        let parent_abs = std::fs::canonicalize(&config_dir).unwrap_or_else(|_| config_dir.clone());

        let include = config.include.as_ref().unwrap();
        assert_eq!(include[0], parent_abs.join("src/**/*").to_string_lossy());
        assert_eq!(include[1], "/already/abs", "absolute selectors untouched");
        let exclude = config.exclude.as_ref().unwrap();
        assert_eq!(
            exclude[0],
            parent_abs.join("node_modules").to_string_lossy()
        );
        let files = config.files.as_ref().unwrap();
        assert_eq!(files[0], parent_abs.join("entry.ts").to_string_lossy());
    }

    #[test]
    fn anchor_inherited_root_selectors_normalizes_dot_segments() {
        // Regression for the false TS18003 on a references-only root that
        // inherits `"include": ["./global.d.ts"]` from a base config (the
        // `mswjs/msw` shape). `Path::join` keeps the leading `./`, producing
        // an unmatchable glob `<dir>/./global.d.ts`; anchoring must collapse
        // `.`/`..` while leaving glob metacharacters intact.
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("project");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("tsconfig.base.json");

        let mut config = TsConfig {
            include: Some(vec![
                "./global.d.ts".to_string(),
                "./src/**/*.ts".to_string(),
            ]),
            exclude: Some(vec!["./node_modules".to_string()]),
            files: Some(vec!["../shared/entry.ts".to_string()]),
            ..Default::default()
        };

        anchor_inherited_root_selectors(&mut config, &config_path);
        let parent_abs = std::fs::canonicalize(&config_dir).unwrap_or_else(|_| config_dir.clone());

        let include = config.include.as_ref().unwrap();
        assert_eq!(
            include[0],
            parent_abs.join("global.d.ts").to_string_lossy(),
            "leading ./ must be collapsed so the glob matches the real file"
        );
        assert_eq!(
            include[1],
            parent_abs.join("src/**/*.ts").to_string_lossy(),
            "glob metacharacters must survive normalization"
        );
        let exclude = config.exclude.as_ref().unwrap();
        assert_eq!(
            exclude[0],
            parent_abs.join("node_modules").to_string_lossy()
        );
        let files = config.files.as_ref().unwrap();
        assert_eq!(
            files[0],
            parent_abs
                .parent()
                .unwrap()
                .join("shared/entry.ts")
                .to_string_lossy(),
            ".. must resolve against the declaring config's directory"
        );

        for selector in include.iter().chain(exclude).chain(files) {
            assert!(
                !selector.contains("/./") && !selector.contains("/../"),
                "anchored selector must not retain dot segments: {selector}"
            );
        }
    }

    #[test]
    fn anchor_inherited_path_options_anchors_baseurl_to_base_dir() {
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("base");
        let dist_dir = config_dir.join("dist");
        std::fs::create_dir_all(&dist_dir).unwrap();
        let config_path = config_dir.join("tsconfig.json");

        let mut config = TsConfig {
            compiler_options: Some(CompilerOptions {
                base_url: Some(".".to_string()),
                out_dir: Some("./dist".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        anchor_inherited_path_options(&mut config, &config_path);

        let opts = config.compiler_options.unwrap();
        let canonical_base =
            std::fs::canonicalize(&config_dir).unwrap_or_else(|_| config_dir.clone());
        let canonical_dist = std::fs::canonicalize(&dist_dir).unwrap_or_else(|_| dist_dir.clone());
        assert_eq!(
            opts.base_url.as_deref(),
            Some(canonical_base.to_string_lossy().as_ref())
        );
        assert_eq!(
            opts.out_dir.as_deref(),
            Some(canonical_dist.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn anchor_inherited_path_options_leaves_absolute_untouched() {
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("base");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("tsconfig.json");

        let abs_path = "/absolute/elsewhere".to_string();
        let mut config = TsConfig {
            compiler_options: Some(CompilerOptions {
                base_url: Some(abs_path.clone()),
                ..Default::default()
            }),
            ..Default::default()
        };

        anchor_inherited_path_options(&mut config, &config_path);

        let opts = config.compiler_options.unwrap();
        assert_eq!(opts.base_url.as_deref(), Some(abs_path.as_str()));
    }

    #[test]
    fn resolve_extends_path_relative() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("base.json"), "{}").unwrap();
        let child = project.join("tsconfig.json");

        // Extensionless relative specifier resolves by appending `.json`.
        let resolved = resolve_extends_path(&child, "./base").unwrap();
        assert_eq!(
            resolved,
            ExtendsResolution::Found(project.join("base.json"))
        );
    }

    #[test]
    fn resolve_extends_path_relative_missing_extensionless_is_not_found() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        // An extensionless relative specifier whose `.json`-appended candidate
        // is also absent is a plain miss: the caller emits the specifier-
        // anchored TS6053, never TS5083.
        let resolved = resolve_extends_path(&child, "./missing").unwrap();
        assert_eq!(resolved, ExtendsResolution::NotFound);
    }

    #[test]
    fn resolve_extends_path_relative_missing_json_is_unreadable() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        // A relative specifier that already ends in `.json` and does not exist
        // resolves to a concrete-but-unreadable path: `tsc` returns it unchecked
        // and the file read fails with TS5083 anchored at the normalized path.
        let resolved = resolve_extends_path(&child, "./nope.json").unwrap();
        assert_eq!(
            resolved,
            ExtendsResolution::Unreadable(project.join("nope.json"))
        );
    }

    #[test]
    fn resolve_extends_path_relative_missing_json_normalizes_parent_segments() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        let nested = project.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let child = nested.join("tsconfig.json");

        // The TS5083 path is lexically normalized: `../nope.json` collapses to
        // the sibling directory, never a `<dir>/../nope.json` spelling.
        let resolved = resolve_extends_path(&child, "../nope.json").unwrap();
        assert_eq!(
            resolved,
            ExtendsResolution::Unreadable(project.join("nope.json"))
        );
    }

    #[test]
    fn resolve_extends_path_relative_missing_non_json_extension_is_not_found() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        // A non-`.json` extension is treated like an extensionless specifier
        // (`tsc` appends `.json` and re-probes), so a miss is TS6053 — TS5083 is
        // reserved for `.json` specifiers only.
        let resolved = resolve_extends_path(&child, "./nope.txt").unwrap();
        assert_eq!(resolved, ExtendsResolution::NotFound);
    }

    #[test]
    fn resolve_extends_path_absolute() {
        let temp = tempdir().unwrap();
        let abs = temp.path().join("abs.json");
        std::fs::write(&abs, "{}").unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, abs.to_string_lossy().as_ref()).unwrap();
        assert_eq!(resolved, ExtendsResolution::Found(abs));
    }

    #[test]
    fn resolve_extends_path_absolute_missing_json_is_unreadable() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");
        let abs_missing = temp.path().join("does").join("not").join("exist.json");

        // A rooted (absolute) `.json` specifier shares the relative branch's
        // TS5083 rule per `isRootedDiskPath` in `commandLineParser.ts`.
        let resolved =
            resolve_extends_path(&child, abs_missing.to_string_lossy().as_ref()).unwrap();
        assert_eq!(resolved, ExtendsResolution::Unreadable(abs_missing));
    }

    #[test]
    fn resolve_extends_path_uses_node_modules_walk() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        let pkg = project.join("node_modules").join("@scope").join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        let base = pkg.join("recommended.json");
        std::fs::write(&base, "{}").unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "@scope/pkg/recommended").unwrap();
        assert_eq!(resolved, ExtendsResolution::Found(base));
    }

    #[test]
    fn resolve_extends_path_uses_node_modules_walk_with_explicit_json() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        let pkg = project.join("node_modules").join("@scope").join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        let base = pkg.join("tsconfig.base.json");
        std::fs::write(&base, "{}").unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "@scope/pkg/tsconfig.base.json").unwrap();
        assert_eq!(resolved, ExtendsResolution::Found(base));
    }

    #[test]
    fn resolve_extends_path_node_modules_walk_from_nested_dir() {
        // The package config lives in the workspace-root `node_modules`, while
        // the consuming config is several directories down (the directus /
        // rocketchat / cal-com monorepo shape). The walk must climb ancestors.
        let temp = tempdir().unwrap();
        let root = temp.path().join("repo");
        let pkg = root.join("node_modules").join("@scope").join("tsconfig");
        std::fs::create_dir_all(&pkg).unwrap();
        let base = pkg.join("node22.json");
        std::fs::write(&base, "{}").unwrap();
        let nested = root.join("apps").join("web");
        std::fs::create_dir_all(&nested).unwrap();
        let child = nested.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "@scope/tsconfig/node22.json").unwrap();
        assert_eq!(resolved, ExtendsResolution::Found(base));
    }

    #[test]
    fn resolve_extends_path_bare_package_uses_root_tsconfig() {
        // A bare package specifier resolves to the package root's tsconfig.json.
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        let pkg = project.join("node_modules").join("shared-config");
        std::fs::create_dir_all(&pkg).unwrap();
        let base = pkg.join("tsconfig.json");
        std::fs::write(&base, "{}").unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "shared-config").unwrap();
        assert_eq!(resolved, ExtendsResolution::Found(base));
    }

    #[test]
    fn resolve_extends_path_missing_package_is_none() {
        // A non-relative specifier whose package is not installed resolves to a
        // miss (caller emits TS6053), never to a literal config-dir path-join.
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "@scope/pkg/file.json").unwrap();
        assert_eq!(resolved, ExtendsResolution::NotFound);
    }
}
