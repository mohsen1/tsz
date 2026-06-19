//! Package exports and imports field resolution.
//!
//! Implements the Node.js `PACKAGE_EXPORTS_RESOLVE` and `PACKAGE_IMPORTS_RESOLVE`
//! algorithms, including conditional exports, pattern matching, and wildcard
//! substitution.

use super::{
    ImportingModuleKind, ModuleExtension, ModuleResolver, ResolutionFailure, ResolvedModule,
};
use crate::config::ModuleResolutionKind;
use crate::module_resolver_helpers::*;
use crate::span::Span;
use std::path::{Component, Path, PathBuf};

/// Returns true when an exports/imports pattern key literally ends with a
/// TypeScript source extension. This mirrors tsc's `resolvedUsingTsExtension`
/// signal: the package author opted into the `.ts` mapping by writing it in
/// the key (e.g. `"./*.ts": ...` or `"#foo.ts": ...`). Wildcard substitutions
/// that happen to capture a `.ts` extension do NOT count — those preserve the
/// user's `.ts` extension through to the resolved target, which is exactly the
/// situation TS2877 warns about.
pub(super) fn key_ends_with_ts_extension(key: &str) -> bool {
    key.ends_with(".ts") || key.ends_with(".tsx") || key.ends_with(".mts") || key.ends_with(".cts")
}

/// Returns true when a conditional `exports`/`imports` key represents a
/// TypeScript types lookup. Matched-key targets in types-flavored branches
/// must go through declaration-aware probing (`try_types_entry`) instead of
/// the runtime probe (`try_export_target`), which under Node16/NodeNext
/// intentionally refuses to add an extension to an extensionless target.
///
/// Recognized: `"types"` (unversioned) and `"types@<range>"` (versioned).
/// `"typings"` is **not** a tsc-recognized exports condition (it's a
/// top-level `package.json` field), so it is not classified here.
pub(super) fn is_types_condition_key(key: &str) -> bool {
    key == "types" || key.starts_with("types@")
}

fn package_relative_target_path(package_dir: &Path, target: &str) -> Option<PathBuf> {
    if !is_valid_relative_package_target(target) {
        return None;
    }
    let rest = target.strip_prefix("./")?;
    Some(package_dir.join(rest))
}

/// Returns true when a relative `exports`/`imports` target string is a valid
/// per-package relative path per Node.js `PACKAGE_TARGET_RESOLVE`.
///
/// A valid relative target:
/// - Starts with `"./"`.
/// - Contains no `..` path segment (cannot escape the package root).
/// - Contains no `node_modules` path segment.
///
/// This is applied AFTER wildcard substitution so that `*` substitutions
/// cannot smuggle in `..` or `node_modules` either.
pub(super) fn is_valid_relative_package_target(target: &str) -> bool {
    if !target.starts_with("./") {
        return false;
    }
    let path = Path::new(target);
    !path.components().any(|component| match component {
        Component::ParentDir | Component::RootDir | Component::Prefix(_) => true,
        Component::Normal(segment) => segment == "node_modules",
        _ => false,
    })
}

/// Returns true when an `imports` target is a valid bare-package specifier
/// per Node.js `PACKAGE_IMPORTS_RESOLVE`.
///
/// A bare specifier must not be empty, must not be absolute (Unix `/...`,
/// Windows backslash, or `<drive>:...`), and must not look relative
/// (`./` / `../`). Bare specifiers are otherwise unrestricted here — full
/// validation happens when the package is resolved.
pub(super) fn is_valid_bare_imports_target(target: &str) -> bool {
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

impl ModuleResolver {
    /// Resolve package.json imports field (#-prefixed specifiers)
    pub(super) fn resolve_package_imports(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
        importing_module_kind: ImportingModuleKind,
        importer_package_type: Option<super::PackageType>,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        let not_found = || ResolutionFailure::NotFound {
            specifier: specifier.to_string(),
            containing_file: containing_file.to_string(),
            span: specifier_span,
        };

        // Per Node.js LOOKUP_PACKAGE_SCOPE + PACKAGE_IMPORTS_RESOLVE (and tsc's
        // `getPackageScopeForPath` / `loadModuleFromImports`), a `#`-prefixed
        // specifier resolves against the SINGLE nearest enclosing package.json —
        // the importer's own package scope. We walk up only to *find* that scope
        // (the importer may live in a subdirectory such as `src/`); once a
        // readable package.json is found we resolve `#imports` against it alone
        // and never fall through to an ancestor package, even when the nearest
        // scope has no `imports` field or no matching key.
        //
        // (An unreadable/invalid package.json is not a usable scope, so we keep
        // walking past it rather than treating a parse failure as a hard wall.)
        let mut current = containing_dir.to_path_buf();
        let (package_json, scope_dir) = loop {
            let package_json_path = current.join("package.json");
            // `read_package_json` caches both hit and miss, so it doubles as the
            // existence check — no separate `cached_is_file` probe needed.
            if let Ok(package_json) = self.read_package_json(&package_json_path) {
                break (package_json, current);
            }
            let Some(parent) = current.parent().filter(|&p| p != current) else {
                return Err(not_found());
            };
            current = parent.to_path_buf();
        };

        let Some(imports) = &package_json.imports else {
            // Nearest package scope defines no `imports` map: fail here without
            // searching ancestor packages, matching tsc.
            return Err(not_found());
        };

        let conditions = self.get_export_conditions(importing_module_kind);
        // `#imports` always resolve inside the importer's own package, so use the
        // scope package.json we just read to anchor the extension priority. The
        // importer's own context is the fallback when it has no `"type"` field.
        let host_pt = self.target_package_type_from_json(&package_json, importer_package_type);

        for (target, resolved_using_ts_extension, is_types_condition) in
            self.resolve_imports_subpath_candidates(imports, specifier, &conditions)
        {
            // Per Node.js PACKAGE_IMPORTS_RESOLVE spec, the resolved
            // (post-substitution) target must either be a relative path within
            // the package (`./...`) or a bare package specifier. Absolute paths
            // and parent escapes are invalid targets and must not resolve.
            if target.starts_with("./") {
                let Some(resolved_path) = package_relative_target_path(&scope_dir, &target) else {
                    continue;
                };
                // Imports targets follow the same PACKAGE_TARGET_RESOLVE
                // algorithm as exports targets, so route them through the
                // shared runtime probe (`try_export_target`) rather than the
                // classic `try_file_or_directory`. This makes an extensionless
                // imports target (e.g. `"#core/*": "./src/core/*"` imported as
                // `#core/sharedOptions`) refuse extension/directory-index
                // addition under Node16/NodeNext/Bundler, matching tsc, while
                // explicit `.js`->`.ts` substitution still applies.
                //
                // A types-flavored condition (`types`/`types@<range>`) keeps
                // declaration-aware probing via `try_types_entry` so an
                // extensionless versioned-types target still finds its `.d.ts`
                // sibling, mirroring the exports path.
                let probed = if is_types_condition {
                    self.try_types_entry(&resolved_path, host_pt)
                        .or_else(|| self.try_export_target(&resolved_path, host_pt))
                } else {
                    self.try_export_target(&resolved_path, host_pt)
                };
                if let Some(resolved) = probed {
                    return Ok(ResolvedModule {
                        resolved_path: resolved.clone(),
                        resolved_using_ts_extension,
                        is_external: false,
                        package_name: package_json.name.clone(),
                        original_specifier: specifier.to_string(),
                        extension: ModuleExtension::from_path(&resolved),
                    });
                }
                continue;
            }

            if !is_valid_bare_imports_target(&target) {
                continue;
            }

            // Bare specifier: resolve as a package (PACKAGE_RESOLVE), supporting
            // self-referencing imports like `"#type": "some-package"`.
            match self.resolve_bare_specifier(
                &target,
                &scope_dir,
                containing_file,
                specifier_span,
                importing_module_kind,
            ) {
                Ok(resolved) => return Ok(resolved),
                Err(
                    ResolutionFailure::NotFound { .. }
                    | ResolutionFailure::AmbiguousProjectRoot { .. },
                ) => continue,
                Err(other) => return Err(other),
            }
        }

        Err(not_found())
    }

    /// Resolve imports field subpath into ordered target candidates.
    ///
    /// Each candidate is `(target, resolved_using_ts_extension, is_types)`. The
    /// `is_types` flag records whether the matched value passed through a
    /// types-flavored condition (`types`/`types@<range>`); the caller uses it to
    /// keep declaration-aware probing for extensionless targets.
    ///
    /// Array targets remain as ordered candidates so filesystem/package
    /// resolution can try later fallbacks when earlier targets are missing.
    fn resolve_imports_subpath_candidates(
        &self,
        imports: &indexmap::IndexMap<String, PackageExports>,
        specifier: &str,
        conditions: &[String],
    ) -> Vec<(String, bool, bool)> {
        // Try exact match first.
        // Keys containing '*' are pattern keys and must not be treated as exact matches.
        if let Some((key, value)) = imports.get_key_value(specifier)
            && !key.contains('*')
        {
            let resolved_using_ts_extension = key_ends_with_ts_extension(key);
            return self
                .resolve_export_targets_to_strings(value, conditions, false)
                .into_iter()
                .map(|(target, is_types)| (target, resolved_using_ts_extension, is_types))
                .collect();
        }

        // Try pattern matching (e.g., "#utils/*").
        // IndexMap guarantees JSON insertion-order, so equal-specificity ties
        // resolve to the first pattern in source order per Node.js/TypeScript spec.
        if let Some((pattern, wildcard, value)) =
            find_best_export_pattern(imports.iter(), |p| match_imports_pattern(p, specifier))
        {
            let resolved_using_ts_extension = key_ends_with_ts_extension(pattern);
            let is_directory_match = pattern.ends_with('/') && !pattern.contains('*');
            return self
                .resolve_export_targets_to_strings(value, conditions, false)
                .into_iter()
                .map(|(target, is_types)| {
                    (
                        apply_wildcard_substitution(&target, &wildcard, is_directory_match),
                        resolved_using_ts_extension,
                        is_types,
                    )
                })
                .collect();
        }

        Vec::new()
    }

    pub(super) fn is_invalid_package_import_specifier(specifier: &str) -> bool {
        specifier == "#" || specifier.starts_with("#/")
    }

    /// Resolve an export/import value to ordered string path candidates.
    ///
    /// Conditional keys are matched via [`Self::condition_key_matches`], which
    /// honors versioned `types@<range>` keys the same way the
    /// `package.json#exports` resolver does. This keeps the imports and
    /// exports paths aligned on conditional matching.
    fn resolve_export_targets_to_strings(
        &self,
        value: &PackageExports,
        conditions: &[String],
        is_types_condition: bool,
    ) -> Vec<(String, bool)> {
        match value {
            PackageExports::String(s) => vec![(s.clone(), is_types_condition)],
            PackageExports::Conditional(cond_entries) => {
                // Iterate condition map entries in JSON key order.
                //
                // The imports path now resolves collected targets through the
                // spec'd runtime probe (`try_export_target`), which refuses to
                // add an extension to an extensionless target under
                // Node16/NodeNext/Bundler. So — exactly like the exports path —
                // it must thread `is_types_condition` so a versioned-types
                // branch (`types`/`types@<range>`) keeps declaration-aware
                // probing for an extensionless target.
                let mut results = Vec::new();
                for (key, nested) in cond_entries {
                    if self.condition_key_matches(key, conditions) {
                        if matches!(nested, PackageExports::Null) {
                            return Vec::new();
                        }
                        let nested_is_types = is_types_condition || is_types_condition_key(key);
                        results.extend(self.resolve_export_targets_to_strings(
                            nested,
                            conditions,
                            nested_is_types,
                        ));
                    }
                }
                results
            }
            PackageExports::Array(elements) => {
                // Array of fallback targets — preserve order so the caller can
                // probe each syntactically applicable target.
                let mut results = Vec::new();
                for element in elements {
                    results.extend(self.resolve_export_targets_to_strings(
                        element,
                        conditions,
                        is_types_condition,
                    ));
                }
                results
            }
            PackageExports::Map(_) | PackageExports::Null => Vec::new(), // Subpath maps not valid here
        }
    }

    /// Get export conditions based on resolution kind and module kind
    ///
    /// Returns conditions in priority order for conditional exports resolution.
    /// The order follows TypeScript 6.0's algorithm:
    /// 1. Custom conditions from tsconfig (prepended to defaults)
    /// 2. "types" - TypeScript always checks this first
    /// 3. Platform condition: "node" for Node-targeted resolution kinds. tsc
    ///    does NOT add "browser" by default for `bundler` mode — `browser`
    ///    must be opted into via `customConditions`.
    /// 4. Primary module condition based on importing file ("import" for ESM, "require" for CJS)
    /// 5. "default" - fallback for unmatched conditions
    pub(super) fn get_export_conditions(
        &self,
        importing_module_kind: ImportingModuleKind,
    ) -> Vec<String> {
        let mut conditions = Vec::new();

        // Custom conditions from tsconfig are prepended to defaults
        for cond in &self.custom_conditions {
            conditions.push(cond.clone());
        }

        // TypeScript always checks "types" first
        conditions.push("types".to_string());

        // Add platform condition: only Node-targeted resolution kinds get "node".
        // Bundler mode does NOT default to "browser" — that must be opted in via
        // `customConditions` (matches tsc 6.0).
        match self.resolution_kind {
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
                conditions.push("node".to_string());
            }
            _ => {}
        }

        // Add module kind condition
        match importing_module_kind {
            ImportingModuleKind::Esm => {
                conditions.push("import".to_string());
            }
            ImportingModuleKind::CommonJs => {
                conditions.push("require".to_string());
            }
        }

        // "default" is always a fallback condition
        conditions.push("default".to_string());

        conditions
    }

    fn condition_key_matches(&self, key: &str, conditions: &[String]) -> bool {
        // Exact-key fast path FIRST: a user-supplied `customConditions` entry
        // can legally contain an `@` (e.g. `"custom@edge"`) and is meant to
        // match its literal spelling, not be parsed as `<base>@<range>`.
        // Falling straight into `parse_condition_key` would split such a key
        // and only match if its base (without `@<rest>`) appears in
        // `conditions`, regressing the pre-PR behavior for any condition
        // whose user-supplied name happens to include `@`.
        if conditions.iter().any(|condition| condition == key) {
            return true;
        }
        let Some((base_condition, version_range)) = key.split_once('@') else {
            return false;
        };
        if !conditions
            .iter()
            .any(|condition| condition == base_condition)
        {
            return false;
        }
        let compiler_version =
            types_versions_compiler_version(self.types_versions_compiler_version.as_deref());
        types_versions_range_matches(version_range, compiler_version)
    }

    /// Resolve package exports with explicit conditions.
    ///
    /// Returns `(resolved_path, resolved_using_ts_extension)`. The bool is `true`
    /// when the matched subpath KEY ends in a TS source extension (e.g. the
    /// author wrote `"./*.ts": "./*.js"`), mirroring tsc's
    /// `resolvedUsingTsExtension` semantics.
    pub(super) fn resolve_package_exports_with_conditions(
        &self,
        package_dir: &Path,
        exports: &PackageExports,
        subpath: &str,
        conditions: &[String],
        is_types_condition: bool,
        target_package_type: Option<super::PackageType>,
    ) -> Option<(PathBuf, bool)> {
        match exports {
            PackageExports::String(s) => {
                if subpath == "." {
                    let resolved = package_relative_target_path(package_dir, s)?;
                    if is_types_condition {
                        if let Some(r) = self
                            .try_types_entry(&resolved, target_package_type)
                            .or_else(|| self.try_export_target(&resolved, target_package_type))
                        {
                            return Some((r, false));
                        }
                    } else if let Some(r) = self.try_export_target(&resolved, target_package_type) {
                        return Some((r, false));
                    }
                }
                None
            }
            PackageExports::Map(map) => {
                // First try exact match.
                // Keys containing '*' are pattern keys and must not be treated as exact matches.
                if let Some((key, value)) = map.get_key_value(subpath)
                    && !key.contains('*')
                {
                    let key_uses_ts = key_ends_with_ts_extension(key);
                    return self
                        .resolve_export_value_with_conditions(
                            package_dir,
                            value,
                            conditions,
                            is_types_condition,
                            target_package_type,
                        )
                        .map(|p| (p, key_uses_ts));
                }

                // Try pattern matching (e.g., "./*" or "./lib/*").
                // IndexMap guarantees JSON insertion-order, so equal-specificity ties
                // resolve to the first pattern in source order per Node.js/TypeScript spec.
                if let Some((pattern, wildcard, value)) =
                    find_best_export_pattern(map.iter(), |p| match_export_pattern(p, subpath))
                {
                    // Per Node.js PACKAGE_TARGET_RESOLVE spec, substitute * with the
                    // matched wildcard portion BEFORE resolving the target path.
                    // Without this, try_export_target would look for literal "*.cjs" files.
                    // Directory-match keys (`./lib/`) also append the wildcard to
                    // `/`-ending targets; `*`-pattern keys (`./*`) don't.
                    let is_directory_match = pattern.ends_with('/') && !pattern.contains('*');
                    let substituted_value =
                        substitute_wildcard_in_exports(value, &wildcard, is_directory_match);
                    let key_uses_ts = key_ends_with_ts_extension(pattern);
                    if let Some(resolved) = self.resolve_export_value_with_conditions(
                        package_dir,
                        &substituted_value,
                        conditions,
                        is_types_condition,
                        target_package_type,
                    ) {
                        return Some((resolved, key_uses_ts));
                    }
                }

                None
            }
            PackageExports::Conditional(cond_entries) => {
                // Iterate condition map entries in JSON key order (not our conditions order)
                for (key, value) in cond_entries {
                    if self.condition_key_matches(key, conditions) {
                        let is_types = is_types_condition || is_types_condition_key(key);
                        // null means explicitly blocked - stop here
                        if matches!(value, PackageExports::Null) {
                            return None;
                        }
                        if let Some(resolved) = self.resolve_package_exports_with_conditions(
                            package_dir,
                            value,
                            subpath,
                            conditions,
                            is_types,
                            target_package_type,
                        ) {
                            return Some(resolved);
                        }
                    }
                }
                None
            }
            PackageExports::Array(elements) => {
                // Array of fallback targets — try each element in order
                for element in elements {
                    if let Some(resolved) = self.resolve_package_exports_with_conditions(
                        package_dir,
                        element,
                        subpath,
                        conditions,
                        is_types_condition,
                        target_package_type,
                    ) {
                        return Some(resolved);
                    }
                }
                None
            }
            PackageExports::Null => None,
        }
    }

    /// Resolve a single export value with conditions.
    ///
    /// This walks the value side of an exports entry only — it does not touch
    /// subpath keys, so it does not contribute to `resolved_using_ts_extension`.
    pub(super) fn resolve_export_value_with_conditions(
        &self,
        package_dir: &Path,
        value: &PackageExports,
        conditions: &[String],
        is_types_condition: bool,
        target_package_type: Option<super::PackageType>,
    ) -> Option<PathBuf> {
        match value {
            PackageExports::String(s) => {
                let resolved = package_relative_target_path(package_dir, s)?;
                if is_types_condition {
                    self.try_types_entry(&resolved, target_package_type)
                        .or_else(|| self.try_export_target(&resolved, target_package_type))
                } else {
                    self.try_export_target(&resolved, target_package_type)
                }
            }
            PackageExports::Conditional(cond_entries) => {
                // Iterate condition map entries in JSON key order
                for (key, nested) in cond_entries {
                    if self.condition_key_matches(key, conditions) {
                        let is_types = is_types_condition || is_types_condition_key(key);
                        // null means explicitly blocked - stop here
                        if matches!(nested, PackageExports::Null) {
                            return None;
                        }
                        if let Some(resolved) = self.resolve_export_value_with_conditions(
                            package_dir,
                            nested,
                            conditions,
                            is_types,
                            target_package_type,
                        ) {
                            return Some(resolved);
                        }
                    }
                }
                None
            }
            PackageExports::Array(elements) => {
                for element in elements {
                    if let Some(resolved) = self.resolve_export_value_with_conditions(
                        package_dir,
                        element,
                        conditions,
                        is_types_condition,
                        target_package_type,
                    ) {
                        return Some(resolved);
                    }
                }
                None
            }
            PackageExports::Map(_) | PackageExports::Null => None,
        }
    }

    /// Resolve a `typesVersions` paths object against a subpath, then probe the
    /// ordered candidate targets on disk via `try_file_or_directory`.
    ///
    /// The version-range selection and exact / longest-prefix pattern matching
    /// (including `*` substitution) are owned by the shared
    /// `tsz_common::module_resolution::types_versions` module, so this resolver
    /// cannot drift from the CLI driver, the checker redirect, or tsc.
    pub(super) fn resolve_types_versions(
        &self,
        package_dir: &Path,
        subpath: &str,
        types_versions: &serde_json::Value,
        target_package_type: Option<super::PackageType>,
    ) -> Option<PathBuf> {
        let compiler_version =
            types_versions_compiler_version(self.types_versions_compiler_version.as_deref());
        let paths = select_types_versions_paths(types_versions, compiler_version)?;
        for target in
            tsz_common::module_resolution::types_versions::candidate_targets(paths, subpath)
        {
            let resolved = package_dir.join(target.trim_start_matches("./"));
            if let Some(found) = self.try_file_or_directory(&resolved, target_package_type) {
                return Some(found);
            }
        }
        None
    }
}
