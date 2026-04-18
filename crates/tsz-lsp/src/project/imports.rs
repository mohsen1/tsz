//! Import candidate collection and auto-import suggestion utilities.
//!
//! Module specifier resolution (computing which path string to use in an import statement)
//! lives in the sibling `module_specifiers` submodule.

use std::path::Path;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::code_actions::{ImportCandidate, ImportCandidateKind};
use crate::completions::{CompletionItem, CompletionItemKind, sort_priority};
use crate::diagnostics::LspDiagnostic;
use crate::symbols::document_symbols::SymbolKind;
use crate::utils::find_node_at_offset;
use tsz_common::position::{Location, Position, Range};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

use super::{ExportMatch, ImportKind, ImportTarget, Project, ProjectFile};

#[derive(Default)]
struct BareSpecifierSourceCache {
    quoted_literal_match: FxHashMap<String, bool>,
    import_like_match: FxHashMap<String, bool>,
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
        seen: &mut FxHashSet<(String, String, String, bool)>,
    ) {
        if !self.auto_imports_allowed_for_file(from_file.file_name()) {
            return;
        }
        let allowed_packages = self.allowed_dependency_package_names(from_file.file_name());
        let existing_imported_packages = Self::imported_package_names(from_file);
        let mut source_cache = BareSpecifierSourceCache::default();
        let mut module_specifiers_cache: FxHashMap<String, Vec<String>> = FxHashMap::default();
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

        for file_name in files_to_check {
            if file_name == from_file.file_name() {
                continue;
            }

            for (module_specifier, export_match) in
                self.matching_exports_in_ambient_modules(&file_name, missing_name)
            {
                if self.is_ambient_module_candidate_excluded(
                    &module_specifier,
                    from_file.source_text(),
                    allowed_packages.as_ref(),
                    &existing_imported_packages,
                    &mut source_cache,
                ) {
                    continue;
                }

                let candidate = ImportCandidate {
                    module_specifier,
                    local_name: missing_name.to_string(),
                    kind: export_match.kind.clone(),
                    is_type_only: export_match.is_type_only,
                };

                let kind_key = match &candidate.kind {
                    ImportCandidateKind::Named { export_name } => format!("named:{export_name}"),
                    ImportCandidateKind::Default => "default".to_string(),
                    ImportCandidateKind::Namespace => "namespace".to_string(),
                };

                if seen.insert((
                    candidate.module_specifier.clone(),
                    candidate.local_name.clone(),
                    kind_key,
                    candidate.is_type_only,
                )) {
                    output.push(candidate);
                }
            }

            let module_specifiers = module_specifiers_cache
                .entry(file_name.clone())
                .or_insert_with(|| {
                    self.auto_import_module_specifiers_from_files(from_file.file_name(), &file_name)
                });
            if module_specifiers.is_empty() {
                continue;
            }

            let mut visited = FxHashSet::default();
            let matches = self.matching_exports_in_file(&file_name, missing_name, &mut visited);
            if matches.is_empty() && !is_namespace_missing {
                continue;
            }

            let Some(module_specifier) = module_specifiers
                .iter()
                .find(|module_specifier| {
                    !self.is_auto_import_candidate_excluded(
                        &file_name,
                        module_specifier,
                        from_file.source_text(),
                        allowed_packages.as_ref(),
                        &existing_imported_packages,
                        &mut source_cache,
                    )
                })
                .cloned()
            else {
                continue;
            };

            for export_match in &matches {
                let candidate = ImportCandidate {
                    module_specifier: module_specifier.clone(),
                    local_name: missing_name.to_string(),
                    kind: export_match.kind.clone(),
                    is_type_only: export_match.is_type_only,
                };

                let kind_key = match &candidate.kind {
                    ImportCandidateKind::Named { export_name } => format!("named:{export_name}"),
                    ImportCandidateKind::Default => "default".to_string(),
                    ImportCandidateKind::Namespace => "namespace".to_string(),
                };

                if seen.insert((
                    candidate.module_specifier.clone(),
                    candidate.local_name.clone(),
                    kind_key,
                    candidate.is_type_only,
                )) {
                    output.push(candidate);
                }
            }

            if is_namespace_missing
                && let Some(is_type_only) = self.export_star_as_default_is_type_only(&file_name)
            {
                let candidate = ImportCandidate {
                    module_specifier: module_specifier.clone(),
                    local_name: missing_name.to_string(),
                    kind: ImportCandidateKind::Default,
                    is_type_only,
                };
                let kind_key = "default".to_string();
                if seen.insert((
                    candidate.module_specifier.clone(),
                    candidate.local_name.clone(),
                    kind_key,
                    candidate.is_type_only,
                )) {
                    output.push(candidate);
                }
            }
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
        seen: &mut FxHashSet<(String, String, String, bool)>,
    ) {
        if !self.auto_imports_allowed_for_file(from_file.file_name()) {
            return;
        }
        let allowed_packages = self.allowed_dependency_package_names(from_file.file_name());
        let existing_imported_packages = Self::imported_package_names(from_file);
        let mut source_cache = BareSpecifierSourceCache::default();
        let mut module_specifiers_cache: FxHashMap<String, Vec<String>> = FxHashMap::default();
        let all_files: Vec<String> = self.files.keys().cloned().collect();
        let wildcard_reexport_files: Vec<String> = all_files
            .iter()
            .filter(|file_name| self.file_has_wildcard_reexport(file_name))
            .cloned()
            .collect();
        let mut supplemental_symbol_set = FxHashSet::default();

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

            for file_name in files_to_check {
                if file_name == from_file.file_name() {
                    continue;
                }

                for (module_specifier, export_match) in
                    self.matching_exports_in_ambient_modules(&file_name, &symbol_name)
                {
                    if self.is_ambient_module_candidate_excluded(
                        &module_specifier,
                        from_file.source_text(),
                        allowed_packages.as_ref(),
                        &existing_imported_packages,
                        &mut source_cache,
                    ) {
                        continue;
                    }

                    let candidate = ImportCandidate {
                        module_specifier,
                        local_name: symbol_name.clone(),
                        kind: export_match.kind.clone(),
                        is_type_only: export_match.is_type_only,
                    };

                    let kind_key = match &candidate.kind {
                        ImportCandidateKind::Named { export_name } => {
                            format!("named:{export_name}")
                        }
                        ImportCandidateKind::Default => "default".to_string(),
                        ImportCandidateKind::Namespace => "namespace".to_string(),
                    };

                    if seen.insert((
                        candidate.module_specifier.clone(),
                        candidate.local_name.clone(),
                        kind_key,
                        candidate.is_type_only,
                    )) {
                        output.push(candidate);
                    }
                }

                let module_specifiers = module_specifiers_cache
                    .entry(file_name.clone())
                    .or_insert_with(|| {
                        self.auto_import_module_specifiers_from_files(
                            from_file.file_name(),
                            &file_name,
                        )
                    });
                if module_specifiers.is_empty() {
                    continue;
                }

                let mut visited = FxHashSet::default();
                let matches = self.matching_exports_in_file(&file_name, &symbol_name, &mut visited);
                if matches.is_empty() {
                    continue;
                }

                let Some(module_specifier) = module_specifiers
                    .iter()
                    .find(|module_specifier| {
                        !self.is_auto_import_candidate_excluded(
                            &file_name,
                            module_specifier,
                            from_file.source_text(),
                            allowed_packages.as_ref(),
                            &existing_imported_packages,
                            &mut source_cache,
                        )
                    })
                    .cloned()
                else {
                    continue;
                };

                for export_match in &matches {
                    let candidate = ImportCandidate {
                        module_specifier: module_specifier.clone(),
                        local_name: symbol_name.clone(),
                        kind: export_match.kind.clone(),
                        is_type_only: export_match.is_type_only,
                    };

                    let kind_key = match &candidate.kind {
                        ImportCandidateKind::Named { export_name } => {
                            format!("named:{export_name}")
                        }
                        ImportCandidateKind::Default => "default".to_string(),
                        ImportCandidateKind::Namespace => "namespace".to_string(),
                    };

                    if seen.insert((
                        candidate.module_specifier.clone(),
                        candidate.local_name.clone(),
                        kind_key,
                        candidate.is_type_only,
                    )) {
                        output.push(candidate);
                    }
                }
            }
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
        let Some(file) = self.files.get(file_name) else {
            return false;
        };
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return false;
        };

        source_file.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                return false;
            }
            let Some(export) = arena.get_export_decl(stmt_node) else {
                return false;
            };
            let is_namespace_reexport = if export.export_clause.is_none() {
                false
            } else if let Some(clause_node) = arena.get(export.export_clause) {
                clause_node.kind == SyntaxKind::Identifier as u16
                    || clause_node.kind == SyntaxKind::StringLiteral as u16
            } else {
                false
            };
            export.module_specifier.is_some()
                && (export.export_clause.is_none() || is_namespace_reexport)
        })
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

    fn auto_import_path_is_excluded(&self, path: &str) -> bool {
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

    fn allowed_dependency_package_names(&self, from_file: &str) -> Option<FxHashSet<String>> {
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

    fn module_specifier_package_name(module_specifier: &str) -> Option<&str> {
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

    fn imported_package_names(file: &ProjectFile) -> FxHashSet<String> {
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

        // Guard against stale parser snapshots in edit-heavy server tests:
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

    fn is_auto_import_candidate_excluded(
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
        ) {
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

    fn is_ambient_module_candidate_excluded(
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
        ) {
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

    pub(crate) fn completion_from_import_candidate(
        &self,
        candidate: &ImportCandidate,
    ) -> CompletionItem {
        let detail = self.auto_import_detail(candidate);
        let documentation = self.auto_import_documentation(candidate);
        let completion_kind = self.auto_import_completion_kind(candidate);

        let mut item = CompletionItem::new(candidate.local_name.clone(), completion_kind)
            .with_detail(detail)
            .with_sort_text(sort_priority::AUTO_IMPORT)
            .with_has_action()
            .with_source(candidate.module_specifier.clone())
            .with_source_display(candidate.module_specifier.clone())
            .with_kind_modifiers("export".to_string());
        if let Some(doc) = documentation {
            item = item.with_documentation(doc);
        }
        item
    }

    fn auto_import_completion_kind(&self, candidate: &ImportCandidate) -> CompletionItemKind {
        match self.symbol_index.get_definition_kind(&candidate.local_name) {
            Some(SymbolKind::Class) => CompletionItemKind::Class,
            Some(SymbolKind::Method) => CompletionItemKind::Method,
            Some(SymbolKind::Property) | Some(SymbolKind::Field) | Some(SymbolKind::Key) => {
                CompletionItemKind::Property
            }
            Some(SymbolKind::Constant | SymbolKind::String | SymbolKind::Number) => {
                CompletionItemKind::Const
            }
            Some(SymbolKind::Constructor) => CompletionItemKind::Constructor,
            Some(SymbolKind::Enum) => CompletionItemKind::Enum,
            Some(SymbolKind::Interface) => CompletionItemKind::Interface,
            Some(SymbolKind::Function) | Some(SymbolKind::Event) | Some(SymbolKind::Operator) => {
                CompletionItemKind::Function
            }
            Some(SymbolKind::Module) | Some(SymbolKind::Namespace) | Some(SymbolKind::Package) => {
                CompletionItemKind::Module
            }
            Some(SymbolKind::TypeParameter) => CompletionItemKind::TypeParameter,
            Some(SymbolKind::Struct) => CompletionItemKind::TypeAlias,
            _ => CompletionItemKind::Variable,
        }
    }

    fn auto_import_detail(&self, candidate: &ImportCandidate) -> String {
        let prefix = if candidate.is_type_only {
            "auto-import type"
        } else {
            "auto-import"
        };

        match candidate.kind {
            ImportCandidateKind::Named { .. } => {
                format!("{} from {}", prefix, candidate.module_specifier)
            }
            ImportCandidateKind::Default => {
                format!("{} default from {}", prefix, candidate.module_specifier)
            }
            ImportCandidateKind::Namespace => {
                format!("{} namespace from {}", prefix, candidate.module_specifier)
            }
        }
    }

    fn auto_import_documentation(&self, candidate: &ImportCandidate) -> Option<String> {
        let import_kw = if candidate.is_type_only {
            "import type"
        } else {
            "import"
        };

        let snippet = match &candidate.kind {
            ImportCandidateKind::Named { export_name } => {
                format!(
                    "{} {{ {} }} from \"{}\";",
                    import_kw, export_name, candidate.module_specifier
                )
            }
            ImportCandidateKind::Default => {
                format!(
                    "{} {} from \"{}\";",
                    import_kw, candidate.local_name, candidate.module_specifier
                )
            }
            ImportCandidateKind::Namespace => {
                format!(
                    "{} * as {} from \"{}\";",
                    import_kw, candidate.local_name, candidate.module_specifier
                )
            }
        };

        Some(snippet)
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
                if export_name == "default" && Self::statement_has_default_modifier(arena, stmt_node) {
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
                        if is_type_only && Self::file_has_type_namespace_import(file, &export_text)
                        {
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
                && export_text == export_name
            {
                matches.push(ExportMatch {
                    kind: ImportCandidateKind::Named {
                        export_name: export_text.to_string(),
                    },
                    is_type_only: export.is_type_only,
                });
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
            syntax_kind_ext::FUNCTION_DECLARATION => {
                arena.get_function(stmt_node).and_then(|data| data.modifiers.as_ref())
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                arena.get_class(stmt_node).and_then(|data| data.modifiers.as_ref())
            }
            syntax_kind_ext::INTERFACE_DECLARATION => arena
                .get_interface(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => arena
                .get_type_alias(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::ENUM_DECLARATION => {
                arena.get_enum(stmt_node).and_then(|data| data.modifiers.as_ref())
            }
            syntax_kind_ext::VARIABLE_STATEMENT => arena
                .get_variable(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::MODULE_DECLARATION => {
                arena.get_module(stmt_node).and_then(|data| data.modifiers.as_ref())
            }
            _ => None,
        }
    }

    fn is_supported_direct_export_declaration_kind(kind: u16) -> bool {
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

    fn statement_is_type_only(kind: u16) -> bool {
        kind == syntax_kind_ext::INTERFACE_DECLARATION || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
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

    fn is_ascii_identifier_start(byte: u8) -> bool {
        byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
    }

    fn is_ascii_identifier_continue(byte: u8) -> bool {
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
mod tests {
    use super::*;

    #[test]
    fn auto_import_prefix_candidates_include_barrel_and_direct_path_variants() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "commonjs",
    "paths": {
      "~/*": ["src/*"]
    }
  }
}"#
            .to_string(),
        );
        project.set_file("/src/dirA/thing1A.ts".to_string(), "Thing".to_string());
        project.set_file(
            "/src/dirB/index.ts".to_string(),
            "export * from \"./thing1B\";\nexport * from \"./thing2B\";\n".to_string(),
        );
        project.set_file(
            "/src/dirB/thing1B.ts".to_string(),
            "export class Thing1B {}".to_string(),
        );
        project.set_file(
            "/src/dirB/thing2B.ts".to_string(),
            "export class Thing2B {}".to_string(),
        );

        let mut thing2_specs: Vec<String> = project
            .get_import_candidates_for_prefix("/src/dirA/thing1A.ts", "Thing")
            .into_iter()
            .filter(|candidate| candidate.local_name == "Thing2B")
            .map(|candidate| candidate.module_specifier)
            .collect();
        thing2_specs.sort();
        thing2_specs.dedup();

        assert_eq!(
            thing2_specs,
            vec!["~/dirB".to_string(), "~/dirB/thing2B".to_string()]
        );
    }

    #[test]
    fn auto_import_candidates_include_ambient_module_exports() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/lib/index.d.ts".to_string(),
            "declare module \"ambient\" { export const x: number; }\ndeclare module \"ambient/utils\" { export const x: number; }\n".to_string(),
        );
        project.set_file("/index.ts".to_string(), "x".to_string());

        let mut specs: Vec<String> = project
            .get_import_candidates_for_prefix("/index.ts", "x")
            .into_iter()
            .map(|candidate| candidate.module_specifier)
            .collect();
        specs.sort();
        specs.dedup();

        assert_eq!(
            specs,
            vec!["ambient".to_string(), "ambient/utils".to_string()]
        );
    }

    #[test]
    fn auto_import_candidates_include_commonjs_exports_from_js_files() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "node18",
    "allowJs": true,
    "checkJs": true
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/matrix.js".to_string(),
            "exports.variants = [];".to_string(),
        );
        project.set_file("/main.js".to_string(), "variants".to_string());

        let specs: Vec<String> = project
            .get_import_candidates_for_prefix("/main.js", "variants")
            .into_iter()
            .filter(|candidate| candidate.local_name == "variants")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|spec| spec == "./matrix.js"),
            "expected './matrix.js' auto-import candidate, got {specs:?}"
        );
    }

    #[test]
    fn auto_import_candidates_include_workspace_file_dependency_alias_specifier() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/solution/packages/utils/package.json".to_string(),
            r#"{
  "name": "utils",
  "version": "1.0.0",
  "exports": "./dist/index.js"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/solution/packages/utils/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "composite": true,
    "module": "nodenext",
    "rootDir": "src",
    "outDir": "dist"
  },
  "include": ["src"]
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/solution/packages/utils/src/index.ts".to_string(),
            "export function gainUtility() { return 0; }".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/solution/packages/web/package.json".to_string(),
            r#"{
  "name": "web",
  "version": "1.0.0",
  "dependencies": {
    "@monorepo/utils": "file:../utils"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/solution/packages/web/src/index.ts".to_string(),
            "gainUtility".to_string(),
        );

        let specs: Vec<String> = project
            .get_import_candidates_for_prefix(
                "/home/src/workspaces/solution/packages/web/src/index.ts",
                "gainUtility",
            )
            .into_iter()
            .filter(|candidate| candidate.local_name == "gainUtility")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|spec| spec == "@monorepo/utils"),
            "expected @monorepo/utils candidate from file-linked workspace dependency, got {specs:?}"
        );
    }

    #[test]
    fn auto_import_candidates_prefer_workspace_dependency_name_for_reexported_symbol() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "mylib": "file:packages/mylib"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/package.json".to_string(),
            r#"{
  "name": "mylib",
  "version": "1.0.0",
  "main": "index.js",
  "types": "index"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
            "export * from \"./mySubDir\";".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/mySubDir/index.ts".to_string(),
            "export * from \"./myClass\";".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/mySubDir/myClass.ts".to_string(),
            "export class MyClass {}".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/src/index.ts".to_string(),
            "const a = new MyClass();".to_string(),
        );

        let specs: Vec<String> = project
            .get_import_candidates_for_prefix(
                "/home/src/workspaces/project/src/index.ts",
                "MyClass",
            )
            .into_iter()
            .filter(|candidate| candidate.local_name == "MyClass")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|spec| spec == "mylib"),
            "expected workspace dependency specifier 'mylib' for re-exported symbol, got {specs:?}"
        );
    }

    #[test]
    fn project_relative_sort_prefers_workspace_dependency_for_reexported_symbol() {
        let mut project = Project::new();
        project.set_import_module_specifier_preference(Some("project-relative".to_string()));
        project.set_file(
            "/home/src/workspaces/project/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "commonjs"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "mylib": "file:packages/mylib"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/package.json".to_string(),
            r#"{
  "name": "mylib",
  "version": "1.0.0",
  "main": "index.js",
  "types": "index"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
            "export * from \"./mySubDir\";".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/mySubDir/index.ts".to_string(),
            "export * from \"./myClass\";".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/mylib/mySubDir/myClass.ts".to_string(),
            "export class MyClass {}".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/src/index.ts".to_string(),
            "const a = new MyClass();".to_string(),
        );

        let mut specs: Vec<String> = project
            .get_import_candidates_for_prefix(
                "/home/src/workspaces/project/src/index.ts",
                "MyClass",
            )
            .into_iter()
            .filter(|candidate| candidate.local_name == "MyClass")
            .map(|candidate| candidate.module_specifier)
            .collect();

        specs.sort_by(|a, b| {
            let a_segments = a.matches('/').count();
            let b_segments = b.matches('/').count();
            let candidate_rank = |candidate: &str| -> u8 {
                if candidate.starts_with("./") {
                    0
                } else if !candidate.starts_with('.') {
                    1
                } else if candidate.starts_with("../") {
                    2
                } else {
                    3
                }
            };
            let index_penalty = |candidate: &str| -> u8 {
                if candidate == "." || candidate == ".." || candidate.ends_with("/index") {
                    1
                } else {
                    0
                }
            };
            a_segments
                .cmp(&b_segments)
                .then_with(|| candidate_rank(a).cmp(&candidate_rank(b)))
                .then_with(|| index_penalty(a).cmp(&index_penalty(b)))
                .then_with(|| a.len().cmp(&b.len()))
                .then_with(|| a.cmp(b))
        });

        assert_eq!(
            specs.first().map(String::as_str),
            Some("mylib"),
            "expected project-relative ordering to still prefer workspace dependency alias, got {specs:?}"
        );
    }

    #[test]
    fn auto_import_candidates_use_type_module_main_subpath_without_index() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/pkg/package.json".to_string(),
            r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib",
  "type": "module"
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/pkg/lib/index.js".to_string(),
            "export function foo() {}".to_string(),
        );
        project.set_file(
            "/package.json".to_string(),
            r#"{
  "dependencies": {
    "pkg": "*"
  }
}"#
            .to_string(),
        );
        project.set_file("/index.ts".to_string(), "foo".to_string());

        let mut specs: Vec<String> = project
            .get_import_candidates_for_prefix("/index.ts", "foo")
            .into_iter()
            .filter(|candidate| candidate.local_name == "foo")
            .map(|candidate| candidate.module_specifier)
            .collect();
        specs.sort();
        specs.dedup();

        assert_eq!(specs, vec!["pkg/lib".to_string()]);
    }

    #[test]
    fn diagnostics_import_candidates_use_type_module_main_subpath_without_index() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/pkg/package.json".to_string(),
            r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib",
  "type": "module"
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/pkg/lib/index.js".to_string(),
            "export function foo() {}".to_string(),
        );
        project.set_file(
            "/package.json".to_string(),
            r#"{
  "dependencies": {
    "pkg": "*"
  }
}"#
            .to_string(),
        );
        project.set_file("/index.ts".to_string(), "foo".to_string());

        let diagnostics = vec![LspDiagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 3)),
            message: "Cannot find name 'foo'.".to_string(),
            code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
            severity: None,
            source: None,
            related_information: None,
            reports_unnecessary: None,
            reports_deprecated: None,
        }];

        let mut specs: Vec<String> = project
            .get_import_candidates_for_diagnostics("/index.ts", &diagnostics)
            .into_iter()
            .filter(|candidate| candidate.local_name == "foo")
            .map(|candidate| candidate.module_specifier)
            .collect();
        specs.sort();
        specs.dedup();

        assert_eq!(specs, vec!["pkg/lib".to_string()]);
    }

    #[test]
    fn auto_import_candidates_include_exports_types_root_and_subpath_entries() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "nodenext"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/package.json".to_string(),
            r#"{
  "dependencies": {
    "dependency": "^1.0.0"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/dependency/package.json".to_string(),
            r#"{
  "type": "module",
  "name": "dependency",
  "version": "1.0.0",
  "exports": {
    ".": { "types": "./lib/index.d.ts" },
    "./lol": { "types": "./lib/lol.d.ts" }
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/dependency/lib/index.d.ts".to_string(),
            "export function fooFromIndex(): void;".to_string(),
        );
        project.set_file(
            "/node_modules/dependency/lib/lol.d.ts".to_string(),
            "export function fooFromLol(): void;".to_string(),
        );
        project.set_file("/src/foo.ts".to_string(), "fooFrom".to_string());

        let candidates = project.get_import_candidates_for_prefix("/src/foo.ts", "fooFrom");
        let specs_for = |name: &str| -> Vec<String> {
            candidates
                .iter()
                .filter(|candidate| candidate.local_name == name)
                .map(|candidate| candidate.module_specifier.clone())
                .collect()
        };

        let index_specs = specs_for("fooFromIndex");
        assert!(
            index_specs
                .iter()
                .any(|specifier| specifier == "dependency"),
            "expected fooFromIndex auto-import from dependency root export-map types entry, got {index_specs:?}"
        );

        let lol_specs = specs_for("fooFromLol");
        assert!(
            lol_specs
                .iter()
                .any(|specifier| specifier == "dependency/lol"),
            "expected fooFromLol auto-import from dependency/lol export-map types entry, got {lol_specs:?}"
        );
    }

    #[test]
    fn auto_import_file_exclude_patterns_hide_store_layout_package_candidates() {
        let mut project = Project::new();
        project.set_auto_import_file_exclude_patterns(vec![
            "/**/@remix-run/server-runtime".to_string(),
        ]);
        project.set_file(
            "/home/src/workspaces/project/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "commonjs"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "@remix-run/server-runtime": "*"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/.store/@remix-run-server-runtime-virtual-c72daf0d/package/package.json".to_string(),
            r#"{
  "name": "@remix-run/server-runtime",
  "version": "0.0.0",
  "main": "index.js"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/.store/@remix-run-server-runtime-virtual-c72daf0d/package/index.d.ts".to_string(),
            "export declare function ServerRuntimeMetaFunction(): void;".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/index.ts".to_string(),
            "ServerRuntimeMetaFunction".to_string(),
        );

        let candidates = project.get_import_candidates_for_prefix(
            "/home/src/workspaces/project/index.ts",
            "ServerRuntimeMetaFunction",
        );
        assert!(
            !candidates
                .iter()
                .any(|candidate| candidate.local_name == "ServerRuntimeMetaFunction"),
            "expected store-layout package candidate to be excluded, got {candidates:?}"
        );
    }

    #[test]
    fn ambient_module_auto_import_candidates_respect_specifier_exclude_regexes() {
        let mut project = Project::new();
        project.set_auto_import_specifier_exclude_regexes(vec!["utils".to_string()]);
        project.set_file(
            "/node_modules/lib/index.d.ts".to_string(),
            "declare module \"ambient\" { export const x: number; }\ndeclare module \"ambient/utils\" { export const x: number; }\n".to_string(),
        );
        project.set_file("/index.ts".to_string(), "x".to_string());

        let mut specs: Vec<String> = project
            .get_import_candidates_for_prefix("/index.ts", "x")
            .into_iter()
            .map(|candidate| candidate.module_specifier)
            .collect();
        specs.sort();
        specs.dedup();

        assert_eq!(specs, vec!["ambient".to_string()]);
    }

    #[test]
    fn ambient_module_auto_import_file_exclude_patterns_are_all_or_nothing() {
        let mut project = Project::new();
        project.set_auto_import_file_exclude_patterns(vec!["/**/ambient1.d.ts".to_string()]);
        project.set_file(
            "/ambient1.d.ts".to_string(),
            "declare module \"foo\" { export const x = 1; }\n".to_string(),
        );
        project.set_file(
            "/ambient2.d.ts".to_string(),
            "declare module \"foo\" { export const y = 2; }\n".to_string(),
        );
        project.set_file("/index.ts".to_string(), "x".to_string());

        let names: FxHashSet<String> = project
            .get_import_candidates_for_prefix("/index.ts", "")
            .into_iter()
            .filter(|candidate| candidate.module_specifier == "foo")
            .map(|candidate| candidate.local_name)
            .collect();

        assert!(
            names.contains("x"),
            "Expected ambient module symbol `x` to remain when only part of a merged ambient module is excluded"
        );
        assert!(
            names.contains("y"),
            "Expected ambient module symbol `y` to remain when only part of a merged ambient module is excluded"
        );
    }

    #[test]
    fn ambient_module_auto_import_file_exclude_patterns_hide_when_all_declarations_excluded() {
        let mut project = Project::new();
        project.set_auto_import_file_exclude_patterns(vec!["/**/ambient*".to_string()]);
        project.set_file(
            "/ambient1.d.ts".to_string(),
            "declare module \"foo\" { export const x = 1; }\n".to_string(),
        );
        project.set_file(
            "/ambient2.d.ts".to_string(),
            "declare module \"foo\" { export const y = 2; }\n".to_string(),
        );
        project.set_file("/index.ts".to_string(), "x".to_string());

        let candidates = project.get_import_candidates_for_prefix("/index.ts", "");
        assert!(
            !candidates
                .iter()
                .any(|candidate| candidate.module_specifier == "foo"),
            "Expected ambient module `foo` to be excluded when all declaration files are excluded"
        );
    }

    #[test]
    fn auto_import_candidates_include_export_equals_identifier_default() {
        let mut project = Project::new();
        project.set_file(
            "/ts.d.ts".to_string(),
            r#"declare namespace ts {
  interface SourceFile {
    text: string;
  }
}
export = ts;
"#
            .to_string(),
        );
        project.set_file("/types.ts".to_string(), "ts".to_string());

        let has_ts_default = project
            .get_import_candidates_for_prefix("/types.ts", "ts")
            .into_iter()
            .any(|candidate| {
                candidate.local_name == "ts"
                    && candidate.module_specifier == "./ts"
                    && matches!(candidate.kind, ImportCandidateKind::Default)
            });

        assert!(
            has_ts_default,
            "expected default auto-import candidate `ts` from `./ts` for `export = ts` declarations"
        );
    }

    #[test]
    fn diagnostics_import_candidates_include_default_from_export_star_as_default() {
        let mut project = Project::new();
        project.set_file("/a.ts".to_string(), "export class A {}\n".to_string());
        project.set_file(
            "/ns.ts".to_string(),
            "export * as default from \"./a\";\n".to_string(),
        );
        project.set_file("/e.ts".to_string(), "let x: ns.A;\n".to_string());

        let diagnostics = vec![LspDiagnostic {
            range: Range::new(Position::new(0, 7), Position::new(0, 9)),
            message: "Cannot find name 'ns'.".to_string(),
            code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE),
            severity: None,
            source: None,
            related_information: None,
            reports_unnecessary: None,
            reports_deprecated: None,
        }];

        let candidates = project.get_import_candidates_for_diagnostics("/e.ts", &diagnostics);
        let has_default_ns = candidates.iter().any(|candidate| {
            candidate.local_name == "ns"
                && candidate.module_specifier == "./ns"
                && matches!(candidate.kind, ImportCandidateKind::Default)
        });

        assert!(
            has_default_ns,
            "expected default import candidate for `ns` from `./ns` when re-exported via `export * as default`, got: {candidates:?}"
        );
    }

    #[test]
    fn auto_import_candidates_include_reexported_type_namespace_name() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "@jest/types": "*",
    "ts-jest": "*"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/@jest/types/package.json".to_string(),
            r#"{
  "name": "@jest/types"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/@jest/types/index.d.ts".to_string(),
            "import type * as Config from \"./Config\";\nexport type { Config };\n".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/@jest/types/Config.d.ts".to_string(),
            "export interface ConfigGlobals { [k: string]: unknown; }\n".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/ts-jest/index.d.ts".to_string(),
            "export {};\ndeclare module \"@jest/types\" {\n  namespace Config { interface ConfigGlobals { \"ts-jest\": any; } }\n}\n".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/index.ts".to_string(),
            "C".to_string(),
        );

        let has_config = project
            .get_import_candidates_for_prefix("/home/src/workspaces/project/index.ts", "C")
            .into_iter()
            .any(|candidate| {
                candidate.local_name == "Config"
                    && candidate.module_specifier == "@jest/types"
                    && matches!(
                        candidate.kind,
                        ImportCandidateKind::Named { ref export_name } if export_name == "Config"
                    )
            });
        assert!(
            has_config,
            "expected re-exported type namespace `Config` auto-import candidate from @jest/types"
        );
    }

    #[test]
    fn diagnostics_import_candidates_include_package_typings_root_specifier() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/node_modules/@angular/forms/package.json".to_string(),
            r#"{
  "name": "@angular/forms",
  "typings": "./forms.d.ts"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/@angular/forms/forms.d.ts".to_string(),
            "export class PatternValidator {}\n".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"]
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "@angular/forms": "*"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/index.ts".to_string(),
            "PatternValidator".to_string(),
        );

        let diagnostics = vec![LspDiagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 16)),
            message: "Cannot find name 'PatternValidator'.".to_string(),
            code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
            severity: None,
            source: None,
            related_information: None,
            reports_unnecessary: None,
            reports_deprecated: None,
        }];

        let specs: Vec<String> = project
            .get_import_candidates_for_diagnostics(
                "/home/src/workspaces/project/index.ts",
                &diagnostics,
            )
            .into_iter()
            .filter(|candidate| candidate.local_name == "PatternValidator")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|specifier| specifier == "@angular/forms"),
            "expected diagnostics auto-import candidate from @angular/forms, got {specs:?}"
        );
    }

    #[test]
    fn diagnostics_import_candidates_include_pnpm_store_dependency_alias() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "commonjs",
    "lib": ["es5"]
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "mobx": "*"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/package.json"
                .to_string(),
            r#"{
  "types": "dist/mobx.d.ts"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist/mobx.d.ts"
                .to_string(),
            "export declare function autorun(): void;\n".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/index.ts".to_string(),
            "autorun".to_string(),
        );

        let diagnostics = vec![LspDiagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 7)),
            message: "Cannot find name 'autorun'.".to_string(),
            code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
            severity: None,
            source: None,
            related_information: None,
            reports_unnecessary: None,
            reports_deprecated: None,
        }];

        let specs: Vec<String> = project
            .get_import_candidates_for_diagnostics(
                "/home/src/workspaces/project/index.ts",
                &diagnostics,
            )
            .into_iter()
            .filter(|candidate| candidate.local_name == "autorun")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|specifier| specifier == "mobx"),
            "expected diagnostics auto-import candidate from pnpm package alias `mobx`, got {specs:?}"
        );
    }

    #[test]
    fn diagnostics_import_candidates_non_relative_pref_keeps_relative_cross_project_specifier() {
        let mut project = Project::new();
        project.set_import_module_specifier_preference(Some("non-relative".to_string()));
        project.set_file(
            "/home/src/workspaces/project/common/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "commonjs",
    "outDir": "dist",
    "composite": true
  },
  "include": ["src"]
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/common/src/MyModule.ts".to_string(),
            "export function square(n: number) { return n * 2; }\n".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/web/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "esnext",
    "moduleResolution": "node",
    "noEmit": true,
    "baseUrl": "."
  },
  "include": ["src"],
  "references": [{ "path": "../common" }]
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/web/src/MyApp.ts".to_string(),
            "import { square } from \"../../common/dist/src/MyModule\";\n".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/web/src/Helper.ts".to_string(),
            "square(2);\n".to_string(),
        );

        let diagnostics = vec![LspDiagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 6)),
            message: "Cannot find name 'square'.".to_string(),
            code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
            severity: None,
            source: None,
            related_information: None,
            reports_unnecessary: None,
            reports_deprecated: None,
        }];

        let specs: Vec<String> = project
            .get_import_candidates_for_diagnostics(
                "/home/src/workspaces/project/web/src/Helper.ts",
                &diagnostics,
            )
            .into_iter()
            .filter(|candidate| candidate.local_name == "square")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs
                .iter()
                .any(|specifier| specifier == "../../common/src/MyModule"),
            "expected diagnostics auto-import candidate ../../common/src/MyModule under non-relative preference, got {specs:?}"
        );
    }

    #[test]
    fn diagnostics_import_candidates_include_symlinked_workspace_dependency_name() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/a/package.json".to_string(),
            r#"{
  "dependencies": {
    "b": "*"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/a/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "commonjs",
    "target": "esnext"
  },
  "references": [{ "path": "../b" }]
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/a/index.ts".to_string(),
            "new Shape".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/b/package.json".to_string(),
            r#"{
  "types": "out/index.d.ts"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/b/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "outDir": "out",
    "composite": true
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/b/index.ts".to_string(),
            "export class Shape {}\n".to_string(),
        );

        let diagnostics = vec![LspDiagnostic {
            range: Range::new(Position::new(0, 4), Position::new(0, 9)),
            message: "Cannot find name 'Shape'.".to_string(),
            code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
            severity: None,
            source: None,
            related_information: None,
            reports_unnecessary: None,
            reports_deprecated: None,
        }];

        let specs: Vec<String> = project
            .get_import_candidates_for_diagnostics(
                "/home/src/workspaces/project/a/index.ts",
                &diagnostics,
            )
            .into_iter()
            .filter(|candidate| candidate.local_name == "Shape")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|specifier| specifier == "b"),
            "expected diagnostics auto-import candidate `b` for referenced workspace package, got {specs:?}"
        );
    }

    #[test]
    fn diagnostics_import_candidates_include_bare_and_deep_paths_for_reexported_types_entry() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{
  "dependencies": {
    "react-hook-form": "*"
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/react-hook-form/package.json".to_string(),
            r#"{
  "types": "dist/index.d.ts"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/react-hook-form/dist/index.d.ts".to_string(),
            "export * from \"./useForm\";\n".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/react-hook-form/dist/useForm.d.ts"
                .to_string(),
            "export declare function useForm(): void;\n".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/index.ts".to_string(),
            "useForm".to_string(),
        );

        let diagnostics = vec![LspDiagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 7)),
            message: "Cannot find name 'useForm'.".to_string(),
            code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
            severity: None,
            source: None,
            related_information: None,
            reports_unnecessary: None,
            reports_deprecated: None,
        }];

        let specs: Vec<String> = project
            .get_import_candidates_for_diagnostics(
                "/home/src/workspaces/project/index.ts",
                &diagnostics,
            )
            .into_iter()
            .filter(|candidate| candidate.local_name == "useForm")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|specifier| specifier == "react-hook-form"),
            "expected bare react-hook-form diagnostics auto-import candidate, got {specs:?}"
        );
        assert!(
            specs
                .iter()
                .any(|specifier| specifier == "react-hook-form/dist/useForm"),
            "expected deep react-hook-form/dist/useForm diagnostics auto-import candidate, got {specs:?}"
        );
    }

    #[test]
    fn auto_import_candidates_include_direct_exported_class_declarations() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/thing2A.ts".to_string(),
            "export class Thing2A {}".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/entry.ts".to_string(),
            "Thing2".to_string(),
        );

        let specs: Vec<String> = project
            .get_import_candidates_for_prefix("/home/src/workspaces/project/entry.ts", "Thing2")
            .into_iter()
            .filter(|candidate| candidate.local_name == "Thing2A")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|specifier| specifier == "./thing2A"),
            "expected direct exported class declaration candidate ./thing2A, got {specs:?}"
        );
    }

    #[test]
    fn auto_import_candidates_include_exported_functions_in_ambient_modules() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/ambient.d.ts".to_string(),
            r#"declare module "fs" {
  export function accessSync(path: string): void;
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/index.ts".to_string(),
            "access".to_string(),
        );

        let specs: Vec<String> = project
            .get_import_candidates_for_prefix("/home/src/workspaces/project/index.ts", "access")
            .into_iter()
            .filter(|candidate| candidate.local_name == "accessSync")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|specifier| specifier == "fs"),
            "expected ambient module export candidate fs, got {specs:?}"
        );
    }

    #[test]
    fn auto_import_candidates_do_not_infer_workspace_package_name_without_requesting_package() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "nodenext",
    "lib": ["es5"]
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/utils/package.json".to_string(),
            r#"{
  "name": "utils",
  "version": "1.0.0",
  "main": "dist/index.js"
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/utils/dist/index.d.ts".to_string(),
            "export const x: number;".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/packages/app/dist/index.d.ts".to_string(),
            "import {} from \"utils\";\nexport const app: number;".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/script.ts".to_string(),
            "import {} from \"./packages/app/dist/index.js\";\nx".to_string(),
        );

        let specs: Vec<String> = project
            .get_import_candidates_for_prefix("/home/src/workspaces/project/script.ts", "x")
            .into_iter()
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            !specs.iter().any(|specifier| specifier == "utils/dist"),
            "did not expect inferred workspace package specifier utils/dist without requesting package metadata, got {specs:?}"
        );
        assert!(
            specs.iter()
                .any(|specifier| specifier == "./packages/utils/dist/index.js"),
            "expected relative candidate ./packages/utils/dist/index.js, got {specs:?}"
        );
    }

    #[test]
    fn auto_import_candidates_include_component_from_react_types_package() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/tsconfig.json".to_string(),
            r#"{ "compilerOptions": { "module": "commonjs", "lib": ["es2019"], "types": ["*"] } }"#
                .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/package.json".to_string(),
            r#"{ "dependencies": { "antd": "*", "react": "*" } }"#.to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/@types/react/index.d.ts".to_string(),
            "export declare function Component(): void;".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/antd/index.d.ts".to_string(),
            "import \"react\";".to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/index.ts".to_string(),
            "Compon".to_string(),
        );

        let specs: Vec<String> = project
            .get_import_candidates_for_prefix("/home/src/workspaces/project/index.ts", "Compon")
            .into_iter()
            .filter(|candidate| candidate.local_name == "Component")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|specifier| specifier == "react"),
            "expected react auto-import candidate for Component, got {specs:?}"
        );
    }

    #[test]
    fn auto_import_candidates_include_ambient_reexport_source_module() {
        let mut project = Project::new();
        project.set_file(
            "/home/src/workspaces/project/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "commonjs",
    "types": ["*"],
    "lib": ["es5"]
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/@types/node/index.d.ts".to_string(),
            r#"declare module "fs" {
  export function accessSync(path: string): void;
}"#
            .to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/node_modules/@types/fs-extra/index.d.ts".to_string(),
            r#"export * from "fs";"#.to_string(),
        );
        project.set_file(
            "/home/src/workspaces/project/index.ts".to_string(),
            "access".to_string(),
        );

        let specs: Vec<String> = project
            .get_import_candidates_for_prefix("/home/src/workspaces/project/index.ts", "access")
            .into_iter()
            .filter(|candidate| candidate.local_name == "accessSync")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|specifier| specifier == "fs"),
            "expected fs ambient module candidate, got {specs:?}"
        );
        assert!(
            specs.iter().any(|specifier| specifier == "fs-extra"),
            "expected fs-extra re-export candidate, got {specs:?}"
        );
    }
}
