//! Import candidate collection and auto-import suggestion utilities.
//!
//! Module specifier resolution (computing which path string to use in an import statement)
//! lives in the sibling `module_specifiers` submodule.

use std::path::Path;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::code_actions::{ImportCandidate, ImportCandidateKind};
use crate::diagnostics::LspDiagnostic;
use crate::utils::find_node_at_offset;
use tsz_common::position::{Location, Position, Range};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

use super::import_collect::{
    AutoImportCandidateContext, ImportCandidateCollectionMode, ImportCandidateKey,
    ImportCandidateSink,
};
use super::{ExportMatch, ImportKind, ImportTarget, Project, ProjectFile};

#[derive(Default)]
pub(super) struct BareSpecifierSourceCache {
    pub(super) quoted_literal_match: FxHashMap<String, bool>,
    pub(super) import_like_match: FxHashMap<String, bool>,
}

impl Project {
    pub(crate) fn definition_from_import(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<Vec<Location>> {
        let target = self.import_target_at_position(file, position)?;
        let resolved = self.resolve_module_specifier(file.file_name(), &target.module_specifier)?;
        let target_file = self.files.get(&resolved)?;

        match target.kind {
            ImportKind::Namespace => {
                let location = target_file.node_location(target_file.root())?;
                Some(vec![location])
            }
            ImportKind::Default => {
                let locations = target_file.export_locations("default");
                if locations.is_empty() {
                    None
                } else {
                    Some(locations)
                }
            }
            ImportKind::Named(name) => {
                let locations = target_file.export_locations(&name);
                if locations.is_empty() {
                    None
                } else {
                    Some(locations)
                }
            }
        }
    }

    pub(crate) fn import_candidates_for_diagnostics(
        &self,
        file: &ProjectFile,
        diagnostics: &[LspDiagnostic],
    ) -> Vec<ImportCandidate> {
        let mut candidates = Vec::new();
        let mut seen = FxHashSet::default();

        for diag in diagnostics {
            let diag_code = diag.code.unwrap_or_default();
            if diag_code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                && diag_code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE
            {
                continue;
            }

            let Some(missing_name) = self.identifier_at_range(file, diag.range) else {
                continue;
            };

            self.collect_import_candidates_for_name_with_mode(
                file,
                &missing_name,
                diag_code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE,
                &mut candidates,
                &mut seen,
            );
        }

        candidates
    }

    fn collect_import_candidates_for_name_with_mode(
        &self,
        from_file: &ProjectFile,
        missing_name: &str,
        is_namespace_missing: bool,
        output: &mut Vec<ImportCandidate>,
        seen: &mut FxHashSet<ImportCandidateKey>,
    ) {
        if !self.auto_imports_allowed_for_file(from_file.file_name()) {
            return;
        }
        let all_files: Vec<String> = self.files.keys().cloned().collect();
        let wildcard_reexport_files: Vec<String> = all_files
            .iter()
            .filter(|file_name| self.file_has_wildcard_reexport(file_name))
            .cloned()
            .collect();

        let files_to_check = self.files_to_check_for_symbol(
            missing_name,
            from_file.file_name(),
            &all_files,
            &wildcard_reexport_files,
        );

        let mut context = AutoImportCandidateContext::new(self, from_file, &all_files);
        let mut sink = ImportCandidateSink::new(output, seen);
        let mode = ImportCandidateCollectionMode {
            include_namespace_default: is_namespace_missing,
        };

        if !self.collect_import_candidates_for_symbol_from_files(
            files_to_check,
            missing_name,
            mode,
            &mut context,
            &mut sink,
        ) {
            let fallback_files = all_files
                .into_iter()
                .filter(|file_name| file_name != from_file.file_name())
                .collect();
            let _ = self.collect_import_candidates_for_symbol_from_files(
                fallback_files,
                missing_name,
                mode,
                &mut context,
                &mut sink,
            );
        }
    }

    /// Collect import candidates for symbols matching a prefix.
    ///
    /// This is used for auto-completion when the user has typed a partial
    /// identifier (e.g., "use" should match "useEffect", "useState", etc.).
    pub(crate) fn collect_import_candidates_for_prefix(
        &self,
        from_file: &ProjectFile,
        prefix: &str,
        existing: &FxHashSet<String>,
        output: &mut Vec<ImportCandidate>,
        seen: &mut FxHashSet<ImportCandidateKey>,
    ) {
        if !self.auto_imports_allowed_for_file(from_file.file_name()) {
            return;
        }
        let all_files: Vec<String> = self.files.keys().cloned().collect();
        let wildcard_reexport_files: Vec<String> = all_files
            .iter()
            .filter(|file_name| self.file_has_wildcard_reexport(file_name))
            .cloned()
            .collect();

        let mut supplemental_symbol_set = FxHashSet::default();
        let mut context = AutoImportCandidateContext::new(self, from_file, &all_files);
        let mut sink = ImportCandidateSink::new(output, seen);
        let mode = ImportCandidateCollectionMode {
            include_namespace_default: false,
        };

        // Get all symbols that match the prefix using the sorted symbol index
        let mut matching_symbols = self.symbol_index.get_symbols_with_prefix(prefix);
        if !prefix.is_empty() {
            let mut known_symbols: FxHashSet<String> = matching_symbols.iter().cloned().collect();
            let mut supplemental_symbols = Vec::new();
            for file_name in &all_files {
                for export_name in self.reexported_names_with_prefix(file_name, prefix) {
                    if known_symbols.insert(export_name.clone()) {
                        supplemental_symbol_set.insert(export_name.clone());
                        supplemental_symbols.push(export_name);
                    }
                }
            }
            supplemental_symbols.sort();
            matching_symbols.extend(supplemental_symbols);
        }

        for symbol_name in matching_symbols {
            // Skip if the symbol already exists in the current file (local definition or imported)
            if existing.contains(&symbol_name) {
                continue;
            }

            let mut files_to_check = if supplemental_symbol_set.contains(&symbol_name) {
                all_files.clone()
            } else {
                self.files_to_check_for_symbol(
                    &symbol_name,
                    from_file.file_name(),
                    &all_files,
                    &wildcard_reexport_files,
                )
            };
            if !files_to_check.is_empty()
                && files_to_check
                    .iter()
                    .all(|file_name| file_name == from_file.file_name())
            {
                files_to_check = all_files.clone();
            }

            let _ = self.collect_import_candidates_for_symbol_from_files(
                files_to_check,
                &symbol_name,
                mode,
                &mut context,
                &mut sink,
            );
        }
    }

    fn collect_import_candidates_for_symbol_from_files(
        &self,
        files_to_check: Vec<String>,
        symbol_name: &str,
        mode: ImportCandidateCollectionMode,
        context: &mut AutoImportCandidateContext<'_>,
        sink: &mut ImportCandidateSink<'_>,
    ) -> bool {
        let before_len = sink.len();

        for file_name in files_to_check {
            if file_name == context.request_file_name() {
                continue;
            }

            self.collect_ambient_import_candidates_for_symbol(
                &file_name,
                symbol_name,
                context,
                sink,
            );

            if context.is_regular_file_excluded(&file_name) {
                continue;
            }

            if !context.has_module_specifiers_for(self, &file_name) {
                continue;
            }

            let mut visited = FxHashSet::default();
            let matches = self.matching_exports_in_file(&file_name, symbol_name, &mut visited);
            if matches.is_empty() && !mode.include_namespace_default {
                continue;
            }

            let Some(module_specifier) = context.first_allowed_module_specifier(self, &file_name)
            else {
                continue;
            };

            for export_match in &matches {
                sink.push(ImportCandidate {
                    module_specifier: module_specifier.clone(),
                    local_name: symbol_name.to_string(),
                    kind: export_match.kind.clone(),
                    is_type_only: export_match.is_type_only,
                });
            }

            if mode.include_namespace_default
                && let Some(is_type_only) = self.export_star_as_default_is_type_only(&file_name)
            {
                sink.push(ImportCandidate {
                    module_specifier,
                    local_name: symbol_name.to_string(),
                    kind: ImportCandidateKind::Default,
                    is_type_only,
                });
            }
        }

        sink.len() > before_len
    }

    fn collect_ambient_import_candidates_for_symbol(
        &self,
        file_name: &str,
        symbol_name: &str,
        context: &mut AutoImportCandidateContext<'_>,
        sink: &mut ImportCandidateSink<'_>,
    ) {
        for (module_specifier, export_match) in
            self.matching_exports_in_ambient_modules(file_name, symbol_name)
        {
            if context.is_ambient_module_candidate_excluded(self, &module_specifier) {
                continue;
            }

            sink.push(ImportCandidate {
                module_specifier,
                local_name: symbol_name.to_string(),
                kind: export_match.kind.clone(),
                is_type_only: export_match.is_type_only,
            });
        }
    }

    fn files_to_check_for_symbol(
        &self,
        symbol_name: &str,
        from_file_name: &str,
        all_files: &[String],
        wildcard_reexport_files: &[String],
    ) -> Vec<String> {
        let candidate_files = self.symbol_index.get_files_with_symbol(symbol_name);
        let has_external_candidates = candidate_files
            .iter()
            .any(|file_name| file_name != from_file_name);
        if candidate_files.is_empty() || !has_external_candidates {
            return all_files.to_vec();
        }

        let mut seen = FxHashSet::default();
        let mut files_to_check = Vec::new();

        for file_name in candidate_files
            .into_iter()
            .chain(wildcard_reexport_files.iter().cloned())
        {
            if seen.insert(file_name.clone()) {
                files_to_check.push(file_name);
            }
        }

        files_to_check
    }

    fn file_has_wildcard_reexport(&self, file_name: &str) -> bool {
        self.files
            .get(file_name)
            .is_some_and(|f| f.has_wildcard_reexport)
    }

    fn reexported_names_with_prefix(&self, file_name: &str, prefix: &str) -> Vec<String> {
        let Some(file) = self.files.get(file_name) else {
            return Vec::new();
        };
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return Vec::new();
        };

        let mut names = FxHashSet::default();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                let Some(export_assign) = arena.get_export_assignment(stmt_node) else {
                    continue;
                };
                if export_assign.is_export_equals {
                    if let Some(expr_text) = arena.get_identifier_text(export_assign.expression)
                        && expr_text.starts_with(prefix)
                    {
                        names.insert(expr_text.to_string());
                    }
                } else if "default".starts_with(prefix) {
                    names.insert("default".to_string());
                }
                continue;
            }

            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = arena.get_export_decl(stmt_node) else {
                continue;
            };

            if export.is_default_export && "default".starts_with(prefix) {
                names.insert("default".to_string());
            }

            let clause_idx = export.export_clause;
            if !clause_idx.is_some() {
                continue;
            }
            let Some(clause_node) = arena.get(clause_idx) else {
                continue;
            };

            if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                let Some(named) = arena.get_named_imports(clause_node) else {
                    continue;
                };
                for &spec_idx in &named.elements.nodes {
                    let Some(spec) = arena.get_specifier_at(spec_idx) else {
                        continue;
                    };
                    let export_ident = if spec.name.is_some() {
                        spec.name
                    } else {
                        spec.property_name
                    };
                    let Some(export_text) = arena.get_identifier_text(export_ident) else {
                        continue;
                    };
                    if export_text.starts_with(prefix) {
                        names.insert(export_text.to_string());
                    }
                }
                continue;
            }

            if clause_node.kind == SyntaxKind::Identifier as u16
                && let Some(export_text) = arena.get_identifier_text(clause_idx)
                && export_text.starts_with(prefix)
            {
                names.insert(export_text.to_string());
            }
        }

        let mut out: Vec<String> = names.into_iter().collect();
        out.sort();
        out
    }

    pub(super) fn auto_import_path_is_excluded(&self, path: &str) -> bool {
        if self.auto_import_file_exclude_matchers.is_empty() {
            return false;
        }

        let normalized = path.trim().replace('\\', "/");
        if normalized.is_empty() {
            return false;
        }

        let trimmed = normalized.trim_start_matches('/');
        self.auto_import_file_exclude_matchers
            .iter()
            .any(|matcher| {
                matcher.is_match(&normalized)
                    || (!trimmed.is_empty() && matcher.is_match(trimmed))
                    || normalized
                        .strip_prefix('/')
                        .is_some_and(|stripped| matcher.is_match(stripped))
            })
    }

    fn auto_import_specifier_is_excluded(&self, module_specifier: &str) -> bool {
        self.auto_import_specifier_exclude_matchers
            .iter()
            .any(|matcher| matcher.is_match(module_specifier))
    }

    pub(super) fn allowed_dependency_package_names(
        &self,
        from_file: &str,
    ) -> Option<FxHashSet<String>> {
        let mut allowed = FxHashSet::default();
        let mut saw_package_json = false;
        let mut current = Path::new(from_file).parent();
        while let Some(dir) = current {
            let package_json_path = dir.join("package.json");
            let package_json_key = package_json_path.to_string_lossy().replace('\\', "/");
            let package_json_text = self
                .files
                .get(&package_json_key)
                .map(|f| f.source_text().to_string());

            if let Some(text) = package_json_text {
                saw_package_json = true;
                let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
                    // Match tsserver behavior: invalid package.json should not
                    // suppress auto-import candidates.
                    return None;
                };
                for field in [
                    "dependencies",
                    "devDependencies",
                    "peerDependencies",
                    "optionalDependencies",
                ] {
                    if let Some(deps) = json.get(field).and_then(serde_json::Value::as_object) {
                        allowed.extend(deps.keys().cloned());
                    }
                }
            }

            current = dir.parent();
        }

        saw_package_json.then_some(allowed)
    }

    pub(super) fn module_specifier_package_name(module_specifier: &str) -> Option<&str> {
        if module_specifier.is_empty()
            || module_specifier.starts_with('.')
            || module_specifier.starts_with('/')
            || module_specifier.starts_with('#')
        {
            return None;
        }

        if let Some(scoped) = module_specifier.strip_prefix('@') {
            let mut parts = scoped.split('/');
            let scope = parts.next()?;
            let pkg = parts.next()?;
            if scope.is_empty() || pkg.is_empty() {
                return None;
            }
            let len = 1 + scope.len() + 1 + pkg.len();
            return module_specifier.get(..len);
        }

        module_specifier.split('/').next()
    }

    /// Returns `true` when `position` falls inside a `NamedImports` node —
    /// i.e., the cursor is in the `{ … }` binding list of an `import` statement.
    ///
    /// TypeScript calls this the "import statement completion" context and uses
    /// `SortText.LocationPriority` ("11") instead of `SortText.AutoImportSuggestions`
    /// ("16") for candidates offered there.
    pub(crate) fn is_in_named_import_bindings(file: &ProjectFile, position: Position) -> bool {
        let arena = file.arena();
        let source_text = file.source_text();
        let Some(offset) = file.line_map().position_to_offset(position, source_text) else {
            return false;
        };

        let mut node_idx = find_node_at_offset(arena, offset);
        if node_idx.is_none() && offset > 0 {
            node_idx = find_node_at_offset(arena, offset - 1);
        }

        // Walk up the parent chain until we hit a NAMED_IMPORTS node (found) or
        // pass the statement boundary (IMPORT_DECLARATION / SOURCE_FILE).
        // Bounded to avoid pathological cycles; import nesting is always shallow.
        let mut current = node_idx;
        for _ in 0..8 {
            let Some(node) = arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::NAMED_IMPORTS {
                return true;
            }
            if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || node.kind == syntax_kind_ext::SOURCE_FILE
            {
                break;
            }
            let Some(parent) = arena.parent_of(current) else {
                break;
            };
            current = parent;
        }

        false
    }

    pub(super) fn imported_package_names(file: &ProjectFile) -> FxHashSet<String> {
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return FxHashSet::default();
        };
        let mut imported = FxHashSet::default();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            let module_specifier = if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                arena
                    .get_import_decl(stmt_node)
                    .and_then(|import| arena.get_literal_text(import.module_specifier))
            } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                arena
                    .get_export_decl(stmt_node)
                    .and_then(|export| export.module_specifier.into_option())
                    .and_then(|specifier| arena.get_literal_text(specifier))
            } else {
                None
            };

            if let Some(package_name) = module_specifier
                .and_then(Self::module_specifier_package_name)
                .map(str::to_string)
            {
                imported.insert(package_name);
            }
        }

        imported
    }

    fn source_contains_quoted_package_literal(source_text: &str, package_name: &str) -> bool {
        source_text.contains(&format!("\"{package_name}\""))
            || source_text.contains(&format!("'{package_name}'"))
    }

    fn source_contains_import_like_package_usage(source_text: &str, package_name: &str) -> bool {
        source_text.contains(&format!("from \"{package_name}\""))
            || source_text.contains(&format!("from '{package_name}'"))
            || source_text.contains(&format!("require(\"{package_name}\")"))
            || source_text.contains(&format!("require('{package_name}')"))
            || source_text.contains(&format!("import(\"{package_name}\")"))
            || source_text.contains(&format!("import('{package_name}')"))
            || source_text.contains(&format!("types=\"{package_name}\""))
            || source_text.contains(&format!("types='{package_name}'"))
    }

    fn bare_specifier_allowed_for_file(
        &self,
        module_specifier: &str,
        from_source_text: &str,
        allowed_packages: Option<&FxHashSet<String>>,
        existing_imported_packages: &FxHashSet<String>,
        source_cache: &mut BareSpecifierSourceCache,
    ) -> bool {
        let Some(package_name) = Self::module_specifier_package_name(module_specifier) else {
            return true;
        };

        let node_prefixed =
            (!package_name.starts_with("node:")).then(|| format!("node:{package_name}"));
        let node_stripped = package_name.strip_prefix("node:");

        let mut cached_quoted_literal_match = |candidate: &str| {
            if let Some(cached) = source_cache.quoted_literal_match.get(candidate) {
                return *cached;
            }
            let matched = Self::source_contains_quoted_package_literal(from_source_text, candidate);
            source_cache
                .quoted_literal_match
                .insert(candidate.to_string(), matched);
            matched
        };
        let mut cached_import_like_match = |candidate: &str| {
            if let Some(cached) = source_cache.import_like_match.get(candidate) {
                return *cached;
            }
            let matched =
                Self::source_contains_import_like_package_usage(from_source_text, candidate);
            source_cache
                .import_like_match
                .insert(candidate.to_string(), matched);
            matched
        };

        let has_existing_import = existing_imported_packages.contains(package_name)
            || node_prefixed
                .as_deref()
                .is_some_and(|candidate| existing_imported_packages.contains(candidate))
            || node_stripped
                .is_some_and(|candidate| existing_imported_packages.contains(candidate));

        // Guard against stale parser snapshots after incremental edits:
        // only trust existing-import evidence when the package literal is
        // still present in the current source text.
        let quoted_in_source = cached_quoted_literal_match(package_name)
            || node_prefixed
                .as_deref()
                .is_some_and(&mut cached_quoted_literal_match)
            || node_stripped.is_some_and(&mut cached_quoted_literal_match);
        if has_existing_import && quoted_in_source {
            return true;
        }

        let import_like_in_source = cached_import_like_match(package_name)
            || node_prefixed
                .as_deref()
                .is_some_and(&mut cached_import_like_match)
            || node_stripped.is_some_and(&mut cached_import_like_match);
        if import_like_in_source {
            return true;
        }

        allowed_packages
            .map(|allowed| allowed.contains(package_name))
            .unwrap_or(true)
    }

    /// Returns `true` when a bare specifier with a subpath (e.g. `preact/hooks`)
    /// has its own `package.json` in any loaded `node_modules` directory. This
    /// lets tsz suggest `preact/hooks` even when the parent `preact` package is
    /// not listed in the project's dependencies — the subpackage is directly
    /// installed and addressable.
    fn specifier_subpackage_has_own_manifest(&self, module_specifier: &str) -> bool {
        // Only applies to non-scoped specifiers with at least one slash.
        // Scoped packages (@scope/name) are handled normally.
        if module_specifier.starts_with('@') || !module_specifier.contains('/') {
            return false;
        }
        let needle = format!("/node_modules/{module_specifier}/package.json");
        self.files.keys().any(|k| k.ends_with(&needle))
    }

    pub(super) fn is_auto_import_candidate_excluded(
        &self,
        target_file: &str,
        module_specifier: &str,
        from_source_text: &str,
        allowed_packages: Option<&FxHashSet<String>>,
        existing_imported_packages: &FxHashSet<String>,
        source_cache: &mut BareSpecifierSourceCache,
    ) -> bool {
        if self.auto_import_specifier_is_excluded(module_specifier) {
            return true;
        }

        if self.auto_import_path_is_excluded(target_file) {
            return true;
        }

        if module_specifier.starts_with('.') {
            return false;
        }

        if !self.bare_specifier_allowed_for_file(
            module_specifier,
            from_source_text,
            allowed_packages,
            existing_imported_packages,
            source_cache,
        ) && !self.specifier_subpackage_has_own_manifest(module_specifier)
        {
            return true;
        }

        if self.auto_import_path_is_excluded(module_specifier) {
            return true;
        }

        let synthetic_node_modules_path = format!("/node_modules/{module_specifier}");
        self.auto_import_path_is_excluded(&synthetic_node_modules_path)
            || self
                .auto_import_path_is_excluded(synthetic_node_modules_path.trim_start_matches('/'))
    }

    pub(super) fn is_ambient_module_candidate_excluded(
        &self,
        module_specifier: &str,
        from_source_text: &str,
        allowed_packages: Option<&FxHashSet<String>>,
        existing_imported_packages: &FxHashSet<String>,
        source_cache: &mut BareSpecifierSourceCache,
    ) -> bool {
        if self.auto_import_specifier_is_excluded(module_specifier) {
            return true;
        }

        if module_specifier.starts_with('.') {
            return false;
        }

        if !self.bare_specifier_allowed_for_file(
            module_specifier,
            from_source_text,
            allowed_packages,
            existing_imported_packages,
            source_cache,
        ) && !self.specifier_subpackage_has_own_manifest(module_specifier)
        {
            return true;
        }

        if self.auto_import_path_is_excluded(module_specifier) {
            return true;
        }

        let synthetic_node_modules_path = format!("/node_modules/{module_specifier}");
        if self.auto_import_path_is_excluded(&synthetic_node_modules_path)
            || self
                .auto_import_path_is_excluded(synthetic_node_modules_path.trim_start_matches('/'))
        {
            return true;
        }

        self.ambient_module_declarations_all_excluded(module_specifier)
    }

    fn ambient_module_declarations_all_excluded(&self, module_specifier: &str) -> bool {
        let mut found_declaration = false;

        for (file_name, file) in &self.files {
            if !Self::file_declares_ambient_module(file, module_specifier) {
                continue;
            }
            found_declaration = true;
            if !self.auto_import_path_is_excluded(file_name) {
                return false;
            }
        }

        found_declaration
    }

    fn file_declares_ambient_module(file: &ProjectFile, module_specifier: &str) -> bool {
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return false;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module_decl) = arena.get_module(stmt_node) else {
                continue;
            };
            let Some(declared_name) = arena.get_literal_text(module_decl.name) else {
                continue;
            };
            if declared_name == module_specifier {
                return true;
            }
        }

        false
    }

    fn matching_exports_in_file(
        &self,
        file_name: &str,
        export_name: &str,
        visited: &mut FxHashSet<String>,
    ) -> Vec<ExportMatch> {
        if !visited.insert(file_name.to_string()) {
            return Vec::new();
        }

        let Some(file) = self.files.get(file_name) else {
            return Vec::new();
        };
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return Vec::new();
        };

        let mut matches = Vec::new();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION
                && Self::is_supported_direct_export_declaration_kind(stmt_node.kind)
                && Self::statement_has_export_modifier(arena, stmt_node)
            {
                if !Self::statement_text_contains_name(file.source_text(), stmt_node, export_name) {
                    continue;
                }
                if export_name == "default"
                    && Self::statement_has_default_modifier(arena, stmt_node)
                {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Default,
                        is_type_only: Self::statement_is_type_only(stmt_node.kind),
                    });
                    continue;
                }
                if file.declaration_has_name(stmt_idx, export_name) {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_name.to_string(),
                        },
                        is_type_only: Self::statement_is_type_only(stmt_node.kind),
                    });
                }
                continue;
            }
            if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                let Some(export_assign) = arena.get_export_assignment(stmt_node) else {
                    continue;
                };
                if export_assign.is_export_equals
                    && let Some(expr_text) = arena.get_identifier_text(export_assign.expression)
                    && expr_text == export_name
                {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Default,
                        is_type_only: false,
                    });
                }
                continue;
            }
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }

            let Some(export) = arena.get_export_decl(stmt_node) else {
                continue;
            };

            if export.is_default_export {
                matches.push(ExportMatch {
                    kind: ImportCandidateKind::Default,
                    is_type_only: export.is_type_only,
                });
                continue;
            }

            if export.module_specifier.is_none() {
                if export.export_clause.is_none() {
                    continue;
                }

                let Some(clause_node) = arena.get(export.export_clause) else {
                    continue;
                };
                if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                    let Some(named) = arena.get_named_imports(clause_node) else {
                        continue;
                    };
                    for &spec_idx in &named.elements.nodes {
                        let Some(spec) = arena.get_specifier_at(spec_idx) else {
                            continue;
                        };

                        let export_ident = if spec.name.is_some() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        let Some(export_text) = arena.get_identifier_text(export_ident) else {
                            continue;
                        };
                        if export_text == "default" {
                            matches.push(ExportMatch {
                                kind: ImportCandidateKind::Default,
                                is_type_only: export.is_type_only || spec.is_type_only,
                            });
                        }
                        if export_text != export_name {
                            continue;
                        }

                        let is_type_only = export.is_type_only || spec.is_type_only;
                        matches.push(ExportMatch {
                            kind: ImportCandidateKind::Named {
                                export_name: export_text.to_string(),
                            },
                            is_type_only,
                        });
                        if is_type_only && Self::file_has_type_namespace_import(file, export_text) {
                            matches.push(ExportMatch {
                                kind: ImportCandidateKind::Named {
                                    export_name: export_text.to_string(),
                                },
                                is_type_only: false,
                            });
                        }
                    }
                } else if file.declaration_has_name(export.export_clause, export_name) {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_name.to_string(),
                        },
                        is_type_only: export.is_type_only,
                    });
                }

                continue;
            }

            let module_specifier = match arena.get_literal_text(export.module_specifier) {
                Some(text) => text,
                None => continue,
            };
            if export.export_clause.is_none() {
                if export_name == "default" {
                    continue;
                }

                let has_named_export = if let Some(resolved) =
                    self.resolve_module_specifier(file.file_name(), module_specifier)
                {
                    self.file_exports_named(&resolved, export_name, visited)
                } else {
                    self.ambient_module_exports_named(module_specifier, export_name)
                };

                if has_named_export {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_name.to_string(),
                        },
                        is_type_only: export.is_type_only,
                    });
                }

                continue;
            }

            let Some(clause_node) = arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                let Some(named) = arena.get_named_imports(clause_node) else {
                    continue;
                };
                for &spec_idx in &named.elements.nodes {
                    let Some(spec) = arena.get_specifier_at(spec_idx) else {
                        continue;
                    };

                    let export_ident = if spec.name.is_some() {
                        spec.name
                    } else {
                        spec.property_name
                    };
                    let Some(export_text) = arena.get_identifier_text(export_ident) else {
                        continue;
                    };
                    if export_text == "default" {
                        matches.push(ExportMatch {
                            kind: ImportCandidateKind::Default,
                            is_type_only: export.is_type_only || spec.is_type_only,
                        });
                    }
                    if export_text != export_name {
                        continue;
                    }

                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_text.to_string(),
                        },
                        is_type_only: export.is_type_only || spec.is_type_only,
                    });
                }
            } else if clause_node.kind == SyntaxKind::Identifier as u16
                && let Some(export_text) = arena.get_identifier_text(export.export_clause)
            {
                if export_text == "default" {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Default,
                        is_type_only: export.is_type_only,
                    });
                }
                if export_text == export_name {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_text.to_string(),
                        },
                        is_type_only: export.is_type_only,
                    });
                }
            }
        }

        if matches.is_empty()
            && export_name != "default"
            && Self::is_js_like_file(file_name)
            && Self::has_commonjs_named_export(file, export_name)
        {
            matches.push(ExportMatch {
                kind: ImportCandidateKind::Named {
                    export_name: export_name.to_string(),
                },
                is_type_only: false,
            });
        }

        matches
    }

    fn ambient_module_exports_named(&self, module_specifier: &str, export_name: &str) -> bool {
        self.files.keys().any(|file_name| {
            self.matching_exports_in_ambient_modules(file_name, export_name)
                .iter()
                .any(|(ambient_module, export_match)| {
                    ambient_module == module_specifier
                        && matches!(export_match.kind, ImportCandidateKind::Named { .. })
                })
        })
    }

    fn statement_modifiers<'a>(
        arena: &'a NodeArena,
        stmt_node: &'a tsz_parser::parser::node::Node,
    ) -> Option<&'a tsz_parser::parser::base::NodeList> {
        match stmt_node.kind {
            syntax_kind_ext::FUNCTION_DECLARATION => arena
                .get_function(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::CLASS_DECLARATION => arena
                .get_class(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::INTERFACE_DECLARATION => arena
                .get_interface(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => arena
                .get_type_alias(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::ENUM_DECLARATION => arena
                .get_enum(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::VARIABLE_STATEMENT => arena
                .get_variable(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::MODULE_DECLARATION => arena
                .get_module(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            _ => None,
        }
    }

    const fn is_supported_direct_export_declaration_kind(kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::MODULE_DECLARATION
    }

    fn statement_text_contains_name(
        source_text: &str,
        stmt_node: &tsz_parser::parser::node::Node,
        name: &str,
    ) -> bool {
        if name.is_empty() {
            return false;
        }
        let len = source_text.len();
        let start = (stmt_node.pos as usize).min(len);
        let end = (stmt_node.end as usize).min(len);
        if end <= start {
            return false;
        }
        source_text[start..end].contains(name)
    }

    fn statement_has_export_modifier(
        arena: &NodeArena,
        stmt_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let modifiers = Self::statement_modifiers(arena, stmt_node);
        arena.has_modifier_ref(modifiers, SyntaxKind::ExportKeyword)
    }

    fn statement_has_default_modifier(
        arena: &NodeArena,
        stmt_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let modifiers = Self::statement_modifiers(arena, stmt_node);
        arena.has_modifier_ref(modifiers, SyntaxKind::DefaultKeyword)
    }

    const fn statement_is_type_only(kind: u16) -> bool {
        kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
    }

    fn file_has_type_namespace_import(file: &ProjectFile, namespace_name: &str) -> bool {
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return false;
        };

        source_file.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                return false;
            }
            let Some(import_decl) = arena.get_import_decl(stmt_node) else {
                return false;
            };
            let Some(import_clause_node) = arena.get(import_decl.import_clause) else {
                return false;
            };
            let Some(import_clause) = arena.get_import_clause(import_clause_node) else {
                return false;
            };
            if !import_clause.is_type_only || !import_clause.named_bindings.is_some() {
                return false;
            }
            let Some(named_bindings_node) = arena.get(import_clause.named_bindings) else {
                return false;
            };
            if named_bindings_node.kind != syntax_kind_ext::NAMESPACE_IMPORT {
                return false;
            }
            let Some(namespace_import) = arena.get_named_imports(named_bindings_node) else {
                return false;
            };
            arena
                .get_identifier_text(namespace_import.name)
                .is_some_and(|name| name == namespace_name)
        })
    }

    fn has_commonjs_named_export(file: &ProjectFile, export_name: &str) -> bool {
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return false;
        };

        source_file.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                return false;
            }
            let Some(stmt_data) = arena.get_expression_statement(stmt_node) else {
                return false;
            };
            let Some(expr_node) = arena.get(stmt_data.expression) else {
                return false;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return false;
            }
            let Some(binary) = arena.get_binary_expr(expr_node) else {
                return false;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                return false;
            }

            Self::is_commonjs_export_assignment(arena, binary.left, export_name)
        })
    }

    fn is_commonjs_export_assignment(
        arena: &NodeArena,
        left_idx: NodeIndex,
        export_name: &str,
    ) -> bool {
        let Some(left_node) = arena.get(left_idx) else {
            return false;
        };
        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = arena.get_access_expr(left_node) else {
            return false;
        };
        let Some(member_name) = arena.get_identifier_text(access.name_or_argument) else {
            return false;
        };
        member_name == export_name && Self::is_commonjs_exports_target(arena, access.expression)
    }

    fn is_commonjs_exports_target(arena: &NodeArena, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = arena.get(expr_idx) else {
            return false;
        };

        if expr_node.kind == SyntaxKind::Identifier as u16 {
            return arena.get_identifier_text(expr_idx) == Some("exports");
        }

        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(name) = arena.get_identifier_text(access.name_or_argument) else {
            return false;
        };

        if name == "exports" {
            let Some(base_node) = arena.get(access.expression) else {
                return false;
            };
            if base_node.kind == SyntaxKind::Identifier as u16
                && arena.get_identifier_text(access.expression) == Some("module")
            {
                return true;
            }
        }

        Self::is_commonjs_exports_target(arena, access.expression)
    }

    fn is_js_like_file(file_name: &str) -> bool {
        matches!(
            Path::new(file_name)
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase())
                .as_deref(),
            Some("js" | "jsx" | "mjs" | "cjs")
        )
    }

    fn matching_exports_in_ambient_modules(
        &self,
        file_name: &str,
        export_name: &str,
    ) -> Vec<(String, ExportMatch)> {
        let Some(file) = self.files.get(file_name) else {
            return Vec::new();
        };
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return Vec::new();
        };

        let mut matches = Vec::new();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module_decl) = arena.get_module(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(module_decl.name) else {
                continue;
            };
            let Some(module_body_node) = arena.get(module_decl.body) else {
                continue;
            };
            if module_body_node.kind != syntax_kind_ext::MODULE_BLOCK {
                continue;
            }
            let Some(module_block) = arena.get_module_block(module_body_node) else {
                continue;
            };
            let Some(statements) = module_block.statements.as_ref() else {
                continue;
            };

            for &module_stmt_idx in &statements.nodes {
                let Some(module_stmt_node) = arena.get(module_stmt_idx) else {
                    continue;
                };
                if module_stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    if !Self::is_supported_direct_export_declaration_kind(module_stmt_node.kind) {
                        continue;
                    }
                    if !Self::statement_has_export_modifier(arena, module_stmt_node) {
                        continue;
                    }
                    if !Self::statement_text_contains_name(
                        file.source_text(),
                        module_stmt_node,
                        export_name,
                    ) {
                        continue;
                    }
                    if file.declaration_has_name(module_stmt_idx, export_name) {
                        matches.push((
                            module_specifier.to_string(),
                            ExportMatch {
                                kind: ImportCandidateKind::Named {
                                    export_name: export_name.to_string(),
                                },
                                is_type_only: Self::statement_is_type_only(module_stmt_node.kind),
                            },
                        ));
                    }
                    continue;
                }
                let Some(export) = arena.get_export_decl(module_stmt_node) else {
                    continue;
                };
                if export.module_specifier.is_some() {
                    continue;
                }
                if export.is_default_export {
                    matches.push((
                        module_specifier.to_string(),
                        ExportMatch {
                            kind: ImportCandidateKind::Default,
                            is_type_only: export.is_type_only,
                        },
                    ));
                }
                if file.declaration_has_name(export.export_clause, export_name) {
                    matches.push((
                        module_specifier.to_string(),
                        ExportMatch {
                            kind: ImportCandidateKind::Named {
                                export_name: export_name.to_string(),
                            },
                            is_type_only: export.is_type_only,
                        },
                    ));
                }
            }
        }

        matches
    }

    fn file_exports_named(
        &self,
        file_name: &str,
        export_name: &str,
        visited: &mut FxHashSet<String>,
    ) -> bool {
        self.matching_exports_in_file(file_name, export_name, visited)
            .iter()
            .any(|export_match| matches!(export_match.kind, ImportCandidateKind::Named { .. }))
    }

    fn export_star_as_default_is_type_only(&self, file_name: &str) -> Option<bool> {
        let file = self.files.get(file_name)?;
        let arena = file.arena();
        let source_file = arena.get_source_file_at(file.root())?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export.module_specifier.is_none() || export.export_clause.is_none() {
                continue;
            }
            let clause_node = arena.get(export.export_clause)?;
            let export_text = if clause_node.kind == SyntaxKind::Identifier as u16 {
                arena.get_identifier_text(export.export_clause)
            } else if clause_node.kind == SyntaxKind::StringLiteral as u16 {
                arena.get_literal_text(export.export_clause)
            } else {
                None
            };
            if export_text == Some("default") {
                return Some(export.is_type_only);
            }
        }

        None
    }

    fn identifier_at_range(&self, file: &ProjectFile, range: Range) -> Option<String> {
        let start_offset = file
            .line_map()
            .position_to_offset(range.start, file.source_text())?;
        let end_offset = file
            .line_map()
            .position_to_offset(range.end, file.source_text())
            .unwrap_or(start_offset);

        self.identifier_at_offset(file, start_offset)
            .or_else(|| {
                end_offset
                    .checked_sub(1)
                    .and_then(|offset| self.identifier_at_offset(file, offset))
            })
            .or_else(|| {
                start_offset
                    .checked_sub(1)
                    .and_then(|offset| self.identifier_at_offset(file, offset))
            })
            .or_else(|| {
                Self::identifier_text_from_source_span(file.source_text(), start_offset, end_offset)
            })
    }

    fn identifier_at_offset(&self, file: &ProjectFile, offset: u32) -> Option<String> {
        let node_idx = find_node_at_offset(file.arena(), offset);
        let node = file.arena().get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        file.arena()
            .get_identifier_text(node_idx)
            .map(std::string::ToString::to_string)
    }

    fn identifier_text_from_source_span(
        source_text: &str,
        start_offset: u32,
        end_offset: u32,
    ) -> Option<String> {
        let mut probe_offsets = Vec::with_capacity(4);
        probe_offsets.push(start_offset as usize);
        if end_offset > 0 {
            probe_offsets.push((end_offset - 1) as usize);
        }
        if start_offset > 0 {
            probe_offsets.push((start_offset - 1) as usize);
        }
        if end_offset as usize > start_offset as usize {
            probe_offsets
                .push(((start_offset as usize + end_offset as usize) / 2).saturating_sub(1));
        }

        for probe in probe_offsets {
            if let Some(text) = Self::identifier_text_around_offset(source_text, probe) {
                return Some(text);
            }
        }

        None
    }

    fn identifier_text_around_offset(source_text: &str, probe_offset: usize) -> Option<String> {
        let bytes = source_text.as_bytes();
        if bytes.is_empty() {
            return None;
        }

        let mut idx = probe_offset.min(bytes.len() - 1);
        if !Self::is_ascii_identifier_continue(bytes[idx]) {
            if idx > 0 && Self::is_ascii_identifier_continue(bytes[idx - 1]) {
                idx -= 1;
            } else {
                return None;
            }
        }

        let mut start = idx;
        while start > 0 && Self::is_ascii_identifier_continue(bytes[start - 1]) {
            start -= 1;
        }

        let mut end = idx + 1;
        while end < bytes.len() && Self::is_ascii_identifier_continue(bytes[end]) {
            end += 1;
        }

        if start >= end || !Self::is_ascii_identifier_start(bytes[start]) {
            return None;
        }

        source_text
            .get(start..end)
            .map(std::string::ToString::to_string)
    }

    const fn is_ascii_identifier_start(byte: u8) -> bool {
        byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
    }

    const fn is_ascii_identifier_continue(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
    }

    pub(crate) fn identifier_at_position(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<(NodeIndex, String)> {
        let offset = file
            .line_map()
            .position_to_offset(position, file.source_text())?;
        let mut node_idx = find_node_at_offset(file.arena(), offset);
        if node_idx.is_none() && offset > 0 {
            node_idx = find_node_at_offset(file.arena(), offset - 1);
        }
        if node_idx.is_none() {
            return None;
        }

        let node = file.arena().get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let text = file.arena().get_identifier_text(node_idx)?.to_string();
        Some((node_idx, text))
    }

    pub(crate) fn is_member_access_node(&self, arena: &NodeArena, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while current.is_some() {
            let Some(node) = arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::QUALIFIED_NAME
            {
                return true;
            }

            let Some(ext) = arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
        }

        false
    }

    fn import_target_at_position(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<ImportTarget> {
        let offset = file
            .line_map()
            .position_to_offset(position, file.source_text())?;
        let node_idx = find_node_at_offset(file.arena(), offset);
        if node_idx.is_none() {
            return None;
        }
        self.import_target_from_node(file, node_idx)
    }

    fn import_target_from_node(
        &self,
        file: &ProjectFile,
        node_idx: NodeIndex,
    ) -> Option<ImportTarget> {
        let arena = file.arena();
        let mut current = node_idx;
        let mut import_specifier = None;
        let mut import_clause = None;
        let mut import_decl = None;

        while current.is_some() {
            let node = arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::IMPORT_SPECIFIER => {
                    import_specifier = Some(current);
                }
                k if k == syntax_kind_ext::IMPORT_CLAUSE => {
                    import_clause = Some(current);
                }
                k if k == syntax_kind_ext::IMPORT_DECLARATION
                    || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
                {
                    import_decl = Some(current);
                    break;
                }
                _ => {}
            }
            current = arena.get_extended(current)?.parent;
        }

        let import_decl_idx = import_decl?;
        let import_decl = arena.get_import_decl_at(import_decl_idx)?;
        let module_specifier = arena
            .get_literal_text(import_decl.module_specifier)?
            .to_string();

        let kind = if let Some(spec_idx) = import_specifier {
            let spec = arena.get_specifier_at(spec_idx)?;
            let export_ident = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            let export_name = arena.get_identifier_text(export_ident)?.to_string();
            ImportKind::Named(export_name)
        } else if let Some(clause_idx) = import_clause {
            let clause = arena.get_import_clause_at(clause_idx)?;

            if clause.name == node_idx {
                ImportKind::Default
            } else if clause.named_bindings == node_idx || import_decl.module_specifier == node_idx
            {
                ImportKind::Namespace
            } else {
                return None;
            }
        } else if import_decl.module_specifier == node_idx {
            ImportKind::Namespace
        } else {
            return None;
        };

        Some(ImportTarget {
            module_specifier,
            kind,
        })
    }
}

#[cfg(test)]
#[path = "imports/tests.rs"]
mod tests;
