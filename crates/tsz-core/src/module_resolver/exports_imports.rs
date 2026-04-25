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
use std::path::{Path, PathBuf};

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

impl ModuleResolver {
    /// Resolve package.json imports field (#-prefixed specifiers)
    pub(super) fn resolve_package_imports(
        &self,
        specifier: &str,
        containing_dir: &Path,
        containing_file: &str,
        specifier_span: Span,
        importing_module_kind: ImportingModuleKind,
    ) -> Result<ResolvedModule, ResolutionFailure> {
        // Walk up directory tree looking for package.json with imports field
        let mut current = containing_dir.to_path_buf();

        loop {
            let package_json_path = current.join("package.json");

            if package_json_path.is_file()
                && let Ok(package_json) = self.read_package_json(&package_json_path)
                && let Some(imports) = &package_json.imports
            {
                let conditions = self.get_export_conditions(importing_module_kind);

                if let Some((target, resolved_using_ts_extension)) =
                    self.resolve_imports_subpath(imports, specifier, &conditions)
                {
                    // Per Node.js PACKAGE_IMPORTS_RESOLVE spec:
                    // If the target is a bare specifier (not starting with "./" or "/"),
                    // it should be resolved as a package (PACKAGE_RESOLVE), not as a
                    // relative path. This supports self-referencing imports like
                    // "#type": "package" where the imports field maps to a package name.
                    if !target.starts_with("./") && !target.starts_with('/') {
                        return self
                            .resolve_bare_specifier(
                                &target,
                                &current,
                                containing_file,
                                specifier_span,
                                importing_module_kind,
                            )
                            .map_err(|e| match e {
                                ResolutionFailure::NotFound { span, .. }
                                | ResolutionFailure::AmbiguousProjectRoot { span, .. } => {
                                    ResolutionFailure::NotFound {
                                        specifier: specifier.to_string(),
                                        containing_file: containing_file.to_string(),
                                        span,
                                    }
                                }
                                other => other,
                            });
                    }

                    // Resolve the target as a relative path
                    let resolved_path = current.join(target.trim_start_matches("./"));

                    if let Some(resolved) = self.try_file_or_directory(&resolved_path) {
                        return Ok(ResolvedModule {
                            resolved_path: resolved.clone(),
                            resolved_using_ts_extension,
                            is_external: false,
                            package_name: package_json.name.clone(),
                            original_specifier: specifier.to_string(),
                            extension: ModuleExtension::from_path(&resolved),
                        });
                    }
                }
            }

            // Move to parent directory
            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => break,
            }
        }

        Err(ResolutionFailure::NotFound {
            specifier: specifier.to_string(),
            containing_file: containing_file.to_string(),
            span: specifier_span,
        })
    }

    /// Resolve imports field subpath (similar to exports but with # prefix).
    ///
    /// Returns `(resolved_target, resolved_using_ts_extension)`.
    ///
    /// `resolved_using_ts_extension` mirrors tsc's behavior: it is `true` when
    /// the literal pattern key (the package author's declared mapping) ends in
    /// a TypeScript source extension. It is **not** sufficient for the wildcard
    /// substitution to end in `.ts` — that just preserves the user's `.ts`
    /// through the substitution, which means Node would try to load a `.ts`
    /// file at runtime (the situation TS2877 warns about).
    pub(super) fn resolve_imports_subpath(
        &self,
        imports: &rustc_hash::FxHashMap<String, PackageExports>,
        specifier: &str,
        conditions: &[String],
    ) -> Option<(String, bool)> {
        // Try exact match first.
        // Keys containing '*' are pattern keys and must not be treated as exact matches.
        if let Some((key, value)) = imports.get_key_value(specifier)
            && !key.contains('*')
        {
            let resolved_using_ts_extension = key_ends_with_ts_extension(key);
            return Self::resolve_export_target_to_string(value, conditions)
                .map(|target| (target, resolved_using_ts_extension));
        }

        // Try pattern matching (e.g., "#utils/*")
        let mut best_match: Option<(usize, &str, String, &PackageExports)> = None;

        for (pattern, value) in imports {
            if let Some(wildcard) = match_imports_pattern(pattern, specifier) {
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

        if let Some((_, pattern, wildcard, value)) = best_match
            && let Some(target) = Self::resolve_export_target_to_string(value, conditions)
        {
            let resolved_using_ts_extension = key_ends_with_ts_extension(pattern);
            return Some((
                apply_wildcard_substitution(&target, &wildcard),
                resolved_using_ts_extension,
            ));
        }

        None
    }

    pub(super) fn is_invalid_package_import_specifier(specifier: &str) -> bool {
        specifier == "#" || specifier.starts_with("#/")
    }

    /// Resolve an export/import value to a string path
    pub(super) fn resolve_export_target_to_string(
        value: &PackageExports,
        conditions: &[String],
    ) -> Option<String> {
        match value {
            PackageExports::String(s) => Some(s.clone()),
            PackageExports::Conditional(cond_entries) => {
                // Iterate condition map entries in JSON key order
                for (key, nested) in cond_entries {
                    if conditions.iter().any(|c| c == key) {
                        if matches!(nested, PackageExports::Null) {
                            return None;
                        }
                        if let Some(result) =
                            Self::resolve_export_target_to_string(nested, conditions)
                        {
                            return Some(result);
                        }
                    }
                }
                None
            }
            PackageExports::Array(elements) => {
                // Array of fallback targets — try each element in order
                for element in elements {
                    if let Some(result) = Self::resolve_export_target_to_string(element, conditions)
                    {
                        return Some(result);
                    }
                }
                None
            }
            PackageExports::Map(_) | PackageExports::Null => None, // Subpath maps not valid here
        }
    }

    /// Get export conditions based on resolution kind and module kind
    ///
    /// Returns conditions in priority order for conditional exports resolution.
    /// The order follows TypeScript's algorithm:
    /// 1. Custom conditions from tsconfig (prepended to defaults)
    /// 2. "types" - TypeScript always checks this first
    /// 3. Platform condition ("node" for Node.js, "browser" for bundler)
    /// 4. Primary module condition based on importing file ("import" for ESM, "require" for CJS)
    /// 5. "default" - fallback for unmatched conditions
    /// 6. Opposite module condition as fallback (allows ESM-first packages to work with CJS imports)
    /// 7. Additional platform fallbacks
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

        // Add platform condition: Node modes get "node", bundler uses "browser".
        match self.resolution_kind {
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext => {
                conditions.push("node".to_string());
            }
            ModuleResolutionKind::Bundler => {
                conditions.push("browser".to_string());
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
        if conditions.iter().any(|condition| condition == key) {
            return true;
        }

        let Some(at_pos) = key.find('@') else {
            return false;
        };

        let base_condition = &key[..at_pos];
        let version_range = &key[at_pos + 1..];
        if !conditions
            .iter()
            .any(|condition| condition == base_condition)
        {
            return false;
        }

        let compiler_version =
            types_versions_compiler_version(self.types_versions_compiler_version.as_deref());
        match_types_versions_range(version_range, compiler_version).is_some()
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
    ) -> Option<(PathBuf, bool)> {
        match exports {
            PackageExports::String(s) => {
                if subpath == "." {
                    let resolved = package_dir.join(s.trim_start_matches("./"));
                    if let Some(r) = self.try_export_target(&resolved) {
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
                        .resolve_export_value_with_conditions(package_dir, value, conditions)
                        .map(|p| (p, key_uses_ts));
                }

                // Try pattern matching (e.g., "./*" or "./lib/*")
                let mut best_match: Option<(usize, &str, String, &PackageExports)> = None;

                for (pattern, value) in map {
                    if let Some(matched) = match_export_pattern(pattern, subpath) {
                        let specificity = pattern.len();
                        let is_better = match &best_match {
                            None => true,
                            Some((best_len, _, _, _)) => specificity > *best_len,
                        };
                        if is_better {
                            best_match = Some((specificity, pattern.as_str(), matched, value));
                        }
                    }
                }

                if let Some((_, pattern, wildcard, value)) = best_match {
                    // Per Node.js PACKAGE_TARGET_RESOLVE spec, substitute * with the
                    // matched wildcard portion BEFORE resolving the target path.
                    // Without this, try_export_target would look for literal "*.cjs" files.
                    let substituted_value = substitute_wildcard_in_exports(value, &wildcard);
                    let key_uses_ts = key_ends_with_ts_extension(pattern);
                    if let Some(resolved) = self.resolve_export_value_with_conditions(
                        package_dir,
                        &substituted_value,
                        conditions,
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
                        // null means explicitly blocked - stop here
                        if matches!(value, PackageExports::Null) {
                            return None;
                        }
                        if let Some(resolved) = self.resolve_package_exports_with_conditions(
                            package_dir,
                            value,
                            subpath,
                            conditions,
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
    ) -> Option<PathBuf> {
        match value {
            PackageExports::String(s) => {
                let resolved = package_dir.join(s.trim_start_matches("./"));
                self.try_export_target(&resolved)
            }
            PackageExports::Conditional(cond_entries) => {
                // Iterate condition map entries in JSON key order
                for (key, nested) in cond_entries {
                    if self.condition_key_matches(key, conditions) {
                        // null means explicitly blocked - stop here
                        if matches!(nested, PackageExports::Null) {
                            return None;
                        }
                        if let Some(resolved) = self.resolve_export_value_with_conditions(
                            package_dir,
                            nested,
                            conditions,
                        ) {
                            return Some(resolved);
                        }
                    }
                }
                None
            }
            PackageExports::Array(elements) => {
                for element in elements {
                    if let Some(resolved) =
                        self.resolve_export_value_with_conditions(package_dir, element, conditions)
                    {
                        return Some(resolved);
                    }
                }
                None
            }
            PackageExports::Map(_) | PackageExports::Null => None,
        }
    }

    /// Resolve typesVersions field
    pub(super) fn resolve_types_versions(
        &self,
        package_dir: &Path,
        subpath: &str,
        types_versions: &serde_json::Value,
    ) -> Option<PathBuf> {
        let compiler_version =
            types_versions_compiler_version(self.types_versions_compiler_version.as_deref());
        let paths = select_types_versions_paths(types_versions, compiler_version)?;
        let mut best_pattern: Option<&String> = None;
        let mut best_value: Option<&serde_json::Value> = None;
        let mut best_wildcard = String::new();
        let mut best_specificity = 0usize;
        let mut best_len = 0usize;

        for (pattern, value) in paths {
            let Some(wildcard) = match_types_versions_pattern(pattern, subpath) else {
                continue;
            };
            let specificity = types_versions_specificity(pattern);
            let pattern_len = pattern.len();
            let is_better = match best_pattern {
                None => true,
                Some(current) => {
                    specificity > best_specificity
                        || (specificity == best_specificity && pattern_len > best_len)
                        || (specificity == best_specificity
                            && pattern_len == best_len
                            && pattern < current)
                }
            };

            if is_better {
                best_specificity = specificity;
                best_len = pattern_len;
                best_pattern = Some(pattern);
                best_value = Some(value);
                best_wildcard = wildcard;
            }
        }

        let value = best_value?;

        let mut targets = Vec::new();
        match value {
            serde_json::Value::String(value) => targets.push(value.as_str()),
            serde_json::Value::Array(list) => {
                for entry in list {
                    if let Some(value) = entry.as_str() {
                        targets.push(value);
                    }
                }
            }
            _ => {}
        }

        for target in targets {
            let substituted = apply_wildcard_substitution(target, &best_wildcard);
            let resolved = package_dir.join(substituted.trim_start_matches("./"));
            if let Some(resolved) = self.try_file_or_directory(&resolved) {
                return Some(resolved);
            }
        }

        None
    }
}
