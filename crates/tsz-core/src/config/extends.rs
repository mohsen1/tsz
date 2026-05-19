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
    PackageExports, PackageJson, match_export_pattern, parse_package_specifier,
    substitute_wildcard_in_exports,
};

use super::{CompilerOptions, TsConfig};

/// Resolve `extends` to an absolute path on disk.
///
/// Handles four cases in order:
/// 1. relative or absolute paths (`./base.json`, `/abs/base.json`);
/// 2. package specifiers whose `package.json` exports map points to a
///    config file (`pkg/tsconfig.json` -> `node_modules/pkg/configs/...`);
/// 3. package-name extends that walk `node_modules` upward;
/// 4. a final fallback that treats the value as a relative path.
pub(super) fn resolve_extends_path(current_path: &Path, extends: &str) -> Result<PathBuf> {
    let base_dir = current_path
        .parent()
        .ok_or_else(|| anyhow!("tsconfig has no parent directory"))?;

    // Check if this is a relative or absolute path
    if extends.starts_with('.') || extends.starts_with('/') {
        let mut candidate = PathBuf::from(extends);
        if candidate.extension().is_none() {
            candidate.set_extension("json");
        }

        if candidate.is_absolute() {
            return Ok(candidate);
        }
        return Ok(base_dir.join(candidate));
    }

    if let Some(resolved) = resolve_package_extends_path(current_path, extends) {
        return Ok(resolved);
    }

    // Package-name extends (e.g. "@tsconfig/node20/tsconfig.json")
    // Resolve through node_modules, walking up directory ancestors.
    let mut search_dir = base_dir.to_path_buf();
    loop {
        let mut candidate = search_dir.join("node_modules").join(extends);
        if candidate.extension().is_none() {
            candidate.set_extension("json");
        }
        if candidate.exists() {
            return Ok(candidate);
        }
        // Also try the package's tsconfig.json if extends points to a directory
        let dir_candidate = search_dir.join("node_modules").join(extends);
        if dir_candidate.is_dir() {
            let tsconfig_in_dir = dir_candidate.join("tsconfig.json");
            if tsconfig_in_dir.exists() {
                return Ok(tsconfig_in_dir);
            }
        }
        if !search_dir.pop() {
            break;
        }
    }

    // Fallback: treat as relative path (original behavior)
    let mut candidate = PathBuf::from(extends);
    if candidate.extension().is_none() {
        candidate.set_extension("json");
    }
    Ok(base_dir.join(candidate))
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

            let mut best_match: Option<(usize, &str, String, &PackageExports)> = None;
            for (pattern, value) in map {
                if let Some(wildcard) = match_export_pattern(pattern, subpath) {
                    let specificity = pattern.len();
                    let is_better = match &best_match {
                        None => true,
                        Some((best_len, _, _, _)) => specificity > *best_len,
                    };
                    if is_better {
                        best_match = Some((specificity, pattern.as_str(), wildcard, value));
                    }
                }
            }

            if let Some((_, pattern, wildcard, value)) = best_match {
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
    let resolved = package_dir.join(target.trim_start_matches("./"));
    if resolved.is_file() {
        return Some(resolved);
    }
    if resolved.extension().is_none() {
        let json_path = resolved.with_extension("json");
        if json_path.is_file() {
            return Some(json_path);
        }
    }
    if resolved.is_dir() {
        let tsconfig_path = resolved.join("tsconfig.json");
        if tsconfig_path.is_file() {
            return Some(tsconfig_path);
        }
    }
    None
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
    *selector = base_dir.join(candidate).to_string_lossy().into_owned();
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
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, "./base").unwrap();
        assert_eq!(resolved, project.join("base.json"));
    }

    #[test]
    fn resolve_extends_path_absolute() {
        let temp = tempdir().unwrap();
        let abs = temp.path().join("abs.json");
        let project = temp.path().join("p");
        std::fs::create_dir_all(&project).unwrap();
        let child = project.join("tsconfig.json");

        let resolved = resolve_extends_path(&child, abs.to_string_lossy().as_ref()).unwrap();
        assert_eq!(resolved, abs);
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
        assert_eq!(resolved, base);
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
        assert_eq!(resolved, base);
    }
}
