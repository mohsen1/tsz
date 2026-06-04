impl Server {
    pub(crate) fn normalize_mts_auto_import_edit_text(
        file_path: &str,
        kind: tsz::lsp::completions::CompletionItemKind,
        source_text: &str,
        new_text: &str,
    ) -> String {
        if !file_path.ends_with(".mts") {
            return new_text.to_string();
        }

        let mut normalized = new_text.to_string();
        if matches!(
            kind,
            tsz::lsp::completions::CompletionItemKind::Interface
                | tsz::lsp::completions::CompletionItemKind::TypeAlias
        ) && normalized.starts_with("import {")
        {
            normalized = normalized.replacen("import {", "import type {", 1);
        }

        for marker in [" from \"", " from '"] {
            let Some(marker_idx) = normalized.find(marker) else {
                continue;
            };
            let Some(quote) = marker.chars().last() else {
                continue;
            };
            let start = marker_idx + marker.len();
            let rest = &normalized[start..];
            let Some(end_rel) = rest.find(quote) else {
                continue;
            };
            let end = start + end_rel;
            let module_specifier = &normalized[start..end];
            if module_specifier.starts_with('.')
                && Path::new(module_specifier).extension().is_none()
            {
                normalized.replace_range(start..end, &format!("{module_specifier}.js"));
            }
            break;
        }

        if let Some((module_specifier, imports)) =
            Self::parse_named_import_clause(&normalized, "import {", "} from ")
        {
            let type_only_names =
                Self::type_only_named_imports_for_module(source_text, module_specifier);
            if !type_only_names.is_empty() {
                let mut updated_imports = Vec::new();
                let mut seen_imports = std::collections::BTreeSet::new();
                for part in imports
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                {
                    let bare = part.trim_start_matches("type ").trim();
                    if bare.is_empty() {
                        continue;
                    }
                    seen_imports.insert(bare.to_string());
                    if part.starts_with("type ") {
                        updated_imports.push(part.to_string());
                    } else if type_only_names.contains(bare) {
                        updated_imports.push(format!("type {bare}"));
                    } else {
                        updated_imports.push(part.to_string());
                    }
                }
                for type_only_name in type_only_names {
                    if !seen_imports.contains(&type_only_name) {
                        updated_imports.push(format!("type {type_only_name}"));
                    }
                }
                if !updated_imports.is_empty() {
                    normalized = normalized.replacen(
                        &format!("{{ {imports} }}"),
                        &format!("{{ {} }}", updated_imports.join(", ")),
                        1,
                    );
                }
            }
        }

        normalized
    }

    fn normalize_tsserver_newlines_for_file(text: &str, file_path: &str) -> String {
        let normalized = text.replace("\r\n", "\n");
        let prefers_crlf = file_path.contains("/home/src/workspaces/");
        if prefers_crlf {
            normalized.replace('\n', "\r\n")
        } else {
            normalized
        }
    }

    fn parse_named_import_clause<'a>(
        text: &'a str,
        import_prefix: &str,
        import_suffix: &str,
    ) -> Option<(&'a str, &'a str)> {
        let start = text.find(import_prefix)?;
        let after_prefix = &text[start + import_prefix.len()..];
        let close_brace = after_prefix.find(import_suffix)?;
        let imports = &after_prefix[..close_brace].trim();
        let after_imports = &after_prefix[close_brace + import_suffix.len()..];
        for quote in ['"', '\''] {
            if let Some(quote_start) = after_imports.find(quote) {
                let rest = &after_imports[quote_start + 1..];
                if let Some(quote_end) = rest.find(quote) {
                    let module_specifier = &rest[..quote_end];
                    return Some((module_specifier, imports));
                }
            }
        }
        None
    }

    fn type_only_named_imports_for_module(
        source_text: &str,
        module_specifier: &str,
    ) -> std::collections::BTreeSet<String> {
        let mut names = std::collections::BTreeSet::new();
        for line in source_text.lines() {
            if !line.contains("import type {") {
                continue;
            }
            if !(line.contains(&format!("from \"{module_specifier}\""))
                || line.contains(&format!("from '{module_specifier}'")))
            {
                continue;
            }
            let Some(open) = line.find('{') else {
                continue;
            };
            let Some(close) = line[open + 1..].find('}') else {
                continue;
            };
            let raw_names = &line[open + 1..open + 1 + close];
            for raw_name in raw_names.split(',') {
                let trimmed = raw_name.trim().trim_start_matches("type ").trim();
                if !trimmed.is_empty() {
                    names.insert(trimmed.to_string());
                }
            }
        }
        names
    }

    fn find_type_only_named_import_span(
        source_text: &str,
        module_specifier: &str,
    ) -> Option<(u32, u32)> {
        let mut offset = 0u32;
        for line in source_text.split_inclusive('\n') {
            if line.contains("import type {")
                && (line.contains(&format!("from \"{module_specifier}\""))
                    || line.contains(&format!("from '{module_specifier}'")))
            {
                return Some((offset, line.len() as u32));
            }
            offset += line.len() as u32;
        }
        None
    }

    pub(crate) fn auto_import_code_action_description(
        source_text: &str,
        file_path: &str,
        fallback_source: Option<&str>,
        edits: &[tsz::lsp::rename::TextEdit],
        label: &str,
    ) -> String {
        let source = edits
            .iter()
            .find_map(|edit| Self::extract_module_specifier_from_import_text(&edit.new_text))
            .or(fallback_source)
            .map(|source| {
                if file_path.ends_with(".mts")
                    && source.starts_with('.')
                    && !source.ends_with(".js")
                    && !source.ends_with(".jsx")
                    && !source.ends_with(".mjs")
                    && !source.ends_with(".cjs")
                    && !source.ends_with(".ts")
                    && !source.ends_with(".tsx")
                    && !source.ends_with(".mts")
                    && !source.ends_with(".cts")
                {
                    format!("{source}.js")
                } else {
                    source.to_string()
                }
            });
        source
            .map(|source| {
                let has_existing_import = source_text.contains(&format!("from \"{source}\""))
                    || source_text.contains(&format!("from '{source}'"));
                if has_existing_import {
                    format!("Update import from \"{source}\"")
                } else {
                    format!("Add import from \"{source}\"")
                }
            })
            .unwrap_or_else(|| format!("Apply completion for '{label}'"))
    }

    pub(crate) const fn completion_kind_to_str(
        kind: tsz::lsp::completions::CompletionItemKind,
    ) -> &'static str {
        match kind {
            tsz::lsp::completions::CompletionItemKind::Variable => "var",
            tsz::lsp::completions::CompletionItemKind::Const => "const",
            tsz::lsp::completions::CompletionItemKind::Let => "let",
            tsz::lsp::completions::CompletionItemKind::Function => "function",
            tsz::lsp::completions::CompletionItemKind::Class => "class",
            tsz::lsp::completions::CompletionItemKind::Method => "method",
            tsz::lsp::completions::CompletionItemKind::Parameter => "parameter",
            tsz::lsp::completions::CompletionItemKind::Property => "property",
            tsz::lsp::completions::CompletionItemKind::Keyword => "keyword",
            tsz::lsp::completions::CompletionItemKind::Interface => "interface",
            tsz::lsp::completions::CompletionItemKind::Enum => "enum",
            tsz::lsp::completions::CompletionItemKind::TypeAlias => "type",
            tsz::lsp::completions::CompletionItemKind::Module => "module",
            tsz::lsp::completions::CompletionItemKind::TypeParameter => "type parameter",
            tsz::lsp::completions::CompletionItemKind::Constructor => "constructor",
            tsz::lsp::completions::CompletionItemKind::Alias => "alias",
        }
    }

    fn project_completion_items(
        &self,
        file_name: &str,
        position: tsz::lsp::position::Position,
        preferences: Option<&serde_json::Value>,
    ) -> Vec<tsz::lsp::completions::CompletionItem> {
        let include_module_exports =
            Self::bool_pref_or_default(preferences, "includeCompletionsForModuleExports", false);
        let mut tracked_paths = FxHashSet::default();
        tracked_paths.extend(self.open_files.keys().cloned());
        for project_files in self.external_project_files.values() {
            tracked_paths.extend(project_files.iter().cloned());
        }
        tracked_paths.insert(file_name.to_string());

        let allowed_packages =
            include_module_exports.then(|| self.dependency_package_names_for_file(file_name));
        let workspace_prefix = Self::path_workspace_prefix(file_name);
        let mut files = FxHashMap::default();
        for path in tracked_paths {
            let allowed_packages_ref = allowed_packages
                .as_ref()
                .and_then(std::option::Option::as_ref);
            if !Self::should_include_completion_project_path(
                &path,
                file_name,
                workspace_prefix.as_deref(),
                allowed_packages_ref,
            ) {
                continue;
            }
            if let Some(text) = self
                .open_files
                .get(&path)
                .cloned()
                .or_else(|| std::fs::read_to_string(&path).ok())
            {
                files.insert(path, text);
            }
        }

        if !files.contains_key(file_name)
            && let Ok(content) = std::fs::read_to_string(file_name)
        {
            files.insert(file_name.to_string(), content);
        }
        Self::add_project_config_files(&mut files, file_name);
        if include_module_exports {
            let has_node_modules_file = files
                .keys()
                .any(|path| Self::path_is_under_node_modules(path));
            if !has_node_modules_file {
                self.add_dependency_package_files_for_completion(
                    file_name,
                    allowed_packages
                        .as_ref()
                        .and_then(std::option::Option::as_ref),
                    &mut files,
                );
            }
        }
        if files.is_empty() {
            return Vec::new();
        }

        let mut project = Project::new();
        project.set_allow_importing_ts_extensions(self.allow_importing_ts_extensions);
        project.set_auto_imports_allowed_without_tsconfig(
            self.auto_imports_allowed_for_inferred_projects,
        );
        project.set_import_module_specifier_ending(
            Self::string_pref(preferences, "importModuleSpecifierEnding")
                .or_else(|| self.completion_import_module_specifier_ending.clone()),
        );
        project.set_import_module_specifier_preference(
            Self::string_pref(preferences, "importModuleSpecifierPreference")
                .or_else(|| self.import_module_specifier_preference.clone()),
        );
        project.set_auto_import_file_exclude_patterns(
            Self::string_array_pref(preferences, "autoImportFileExcludePatterns")
                .unwrap_or_else(|| self.auto_import_file_exclude_patterns.clone()),
        );
        project.set_auto_import_specifier_exclude_regexes(
            Self::string_array_pref(preferences, "autoImportSpecifierExcludeRegexes")
                .unwrap_or_default(),
        );
        for (path, text) in files {
            project.set_file(path, text);
        }
        project
            .get_completions(file_name, position)
            .unwrap_or_default()
    }

    fn dependency_package_names_for_file(&self, file_name: &str) -> Option<FxHashSet<String>> {
        let mut allowed = FxHashSet::default();
        let mut saw_package_json = false;
        let mut current = Path::new(file_name).parent();

        while let Some(dir) = current {
            let package_json_path = dir.join("package.json");
            let package_json_key = package_json_path.to_string_lossy().replace('\\', "/");
            let package_json_text = self.open_files.get(&package_json_key).cloned();

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

    fn should_include_completion_project_path(
        path: &str,
        current_file: &str,
        workspace_prefix: Option<&str>,
        allowed_packages: Option<&FxHashSet<String>>,
    ) -> bool {
        let path = path.replace('\\', "/");
        let current_file = current_file.replace('\\', "/");
        if path == current_file {
            return true;
        }

        if Self::path_is_under_node_modules(&path) {
            return Self::node_modules_path_matches_allowed_packages(&path, allowed_packages);
        }

        if Self::is_project_config_file(&path) {
            // Always include package.json files outside node_modules: they
            // carry workspace-package metadata (`name`, `exports`, …) that
            // the auto-import specifier resolver needs even for sibling
            // packages that aren't ancestors of the currently-edited file.
            // `tsconfig.json` / `jsconfig.json` stay gated by ancestry.
            if path.ends_with("/package.json") {
                return true;
            }
            return Self::is_config_related_to_file(&path, &current_file);
        }

        workspace_prefix
            .map(|prefix| {
                if prefix == "/" {
                    // Root workspace: include any absolute path that is not under
                    // node_modules (handled above). Using `format!("{prefix}/")`
                    // would yield "//", which never matches real paths like
                    // "/Component.tsx".
                    path.starts_with('/')
                } else {
                    path == prefix || path.starts_with(&format!("{prefix}/"))
                }
            })
            .unwrap_or(true)
    }

    fn is_project_config_file(path: &str) -> bool {
        path.ends_with("/package.json")
            || path.ends_with("/tsconfig.json")
            || path.ends_with("/jsconfig.json")
    }

    fn is_config_related_to_file(config_path: &str, file_name: &str) -> bool {
        let Some(dir) = Path::new(config_path).parent() else {
            return false;
        };
        let dir = dir.to_string_lossy().replace('\\', "/");
        if dir == "/" {
            return file_name.starts_with('/');
        }
        file_name == dir || file_name.starts_with(&format!("{dir}/"))
    }

    fn path_is_under_node_modules(path: &str) -> bool {
        path.contains("/node_modules/")
    }

    fn path_workspace_prefix(file_name: &str) -> Option<String> {
        let normalized = file_name.replace('\\', "/");
        if normalized.starts_with('/') && normalized.matches('/').count() == 1 {
            return Some("/".to_string());
        }
        let segments: Vec<&str> = normalized
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();
        if segments.is_empty() {
            return None;
        }
        if segments.len() == 1 {
            return normalized.starts_with('/').then_some("/".to_string());
        }
        if segments.len() <= 3 {
            return Some(format!("/{}", segments[0]));
        }
        Some(format!("/{}/{}/{}", segments[0], segments[1], segments[2]))
    }

    fn node_modules_path_matches_allowed_packages(
        path: &str,
        allowed_packages: Option<&FxHashSet<String>>,
    ) -> bool {
        let Some(allowed_packages) = allowed_packages else {
            return true;
        };
        if allowed_packages.is_empty() {
            return true;
        }
        let Some(package_name) = Self::package_name_from_node_modules_path(path) else {
            return false;
        };
        if allowed_packages.contains(&package_name) {
            return true;
        }
        Self::types_package_runtime_name(&package_name)
            .is_some_and(|runtime_name| allowed_packages.contains(&runtime_name))
    }

    fn package_name_from_node_modules_path(path: &str) -> Option<String> {
        let normalized = path.replace('\\', "/");
        let idx = normalized.rfind("/node_modules/")?;
        let mut tail = &normalized[idx + "/node_modules/".len()..];
        if tail.starts_with(".pnpm/")
            && let Some(inner_idx) = tail.find("/node_modules/")
        {
            tail = &tail[inner_idx + "/node_modules/".len()..];
        }
        let mut segments = tail.split('/').filter(|segment| !segment.is_empty());
        let first = segments.next()?;
        if first.starts_with('@') {
            let second = segments.next()?;
            Some(format!("{first}/{second}"))
        } else {
            Some(first.to_string())
        }
    }

    fn types_package_runtime_name(package_name: &str) -> Option<String> {
        let rest = package_name.strip_prefix("@types/")?;
        if let Some((scope, name)) = rest.split_once("__") {
            return Some(format!("@{scope}/{name}"));
        }
        Some(rest.to_string())
    }

    fn types_package_name_for(runtime_package_name: &str) -> String {
        if let Some(stripped) = runtime_package_name.strip_prefix('@')
            && let Some((scope, name)) = stripped.split_once('/')
        {
            return format!("@types/{scope}__{name}");
        }
        format!("@types/{runtime_package_name}")
    }

    fn node_modules_roots_for_file(file_name: &str) -> Vec<String> {
        let mut roots = Vec::new();
        let mut current = Path::new(file_name).parent();
        while let Some(dir) = current {
            roots.push(
                dir.join("node_modules")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
            current = dir.parent();
        }
        roots.sort();
        roots.dedup();
        roots
    }

    fn add_dependency_package_files_for_completion(
        &self,
        file_name: &str,
        allowed_packages: Option<&FxHashSet<String>>,
        files: &mut FxHashMap<String, String>,
    ) {
        let Some(allowed_packages) = allowed_packages else {
            return;
        };
        if allowed_packages.is_empty() {
            return;
        }

        let node_modules_roots = Self::node_modules_roots_for_file(file_name);
        if node_modules_roots.is_empty() {
            return;
        }

        let mut dependency_names: Vec<String> = allowed_packages.iter().cloned().collect();
        dependency_names.sort();
        dependency_names.dedup();

        let mut scanned_dirs = FxHashSet::default();
        for node_modules_root in node_modules_roots {
            for dependency_name in &dependency_names {
                if Self::files_already_include_dependency(files, dependency_name) {
                    continue;
                }
                let dependency_dir = format!("{node_modules_root}/{dependency_name}");
                let dependency_dir_path = Path::new(&dependency_dir);
                if !dependency_dir_path.is_dir() {
                    continue;
                }
                if !scanned_dirs.insert(dependency_dir.clone()) {
                    continue;
                }

                if !Self::files_contain_path_prefix(files, &dependency_dir) {
                    Self::add_supported_files_under_dir(dependency_dir_path, files, 256);
                }

                if Self::files_contain_declaration_under_prefix(files, &dependency_dir) {
                    continue;
                }

                let types_package_name = Self::types_package_name_for(dependency_name);
                let types_dir = format!("{node_modules_root}/{types_package_name}");
                let types_dir_path = Path::new(&types_dir);
                if !types_dir_path.is_dir() {
                    continue;
                }
                if !scanned_dirs.insert(types_dir.clone()) {
                    continue;
                }
                if !Self::files_contain_path_prefix(files, &types_dir) {
                    Self::add_supported_files_under_dir(types_dir_path, files, 256);
                }
            }
        }
    }

    fn files_already_include_dependency(
        files: &FxHashMap<String, String>,
        dependency_name: &str,
    ) -> bool {
        files.keys().any(|path| {
            Self::package_name_from_node_modules_path(path).is_some_and(|package_name| {
                package_name == dependency_name
                    || Self::types_package_runtime_name(&package_name)
                        .is_some_and(|runtime| runtime == dependency_name)
            })
        })
    }

    fn files_contain_path_prefix(files: &FxHashMap<String, String>, prefix: &str) -> bool {
        let normalized_prefix = prefix.replace('\\', "/");
        files.keys().any(|path| {
            path == &normalized_prefix || path.starts_with(&format!("{normalized_prefix}/"))
        })
    }

    fn files_contain_declaration_under_prefix(
        files: &FxHashMap<String, String>,
        prefix: &str,
    ) -> bool {
        let normalized_prefix = prefix.replace('\\', "/");
        files.keys().any(|path| {
            (path == &normalized_prefix || path.starts_with(&format!("{normalized_prefix}/")))
                && (path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts"))
        })
    }

    fn add_supported_files_under_dir(
        root: &Path,
        files: &mut FxHashMap<String, String>,
        max_files: usize,
    ) {
        let mut added = 0usize;
        let mut stack = vec![PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                if file_type.is_dir() {
                    let name = entry.file_name();
                    if name.to_string_lossy().as_ref() != "node_modules" {
                        stack.push(path);
                    }
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }

                let path_str = path.to_string_lossy().replace('\\', "/");
                if files.contains_key(&path_str)
                    || !Self::is_supported_completion_project_file(&path_str)
                {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    files.insert(path_str, text);
                    added += 1;
                    if added >= max_files {
                        return;
                    }
                }
            }
        }
    }

    fn is_supported_completion_project_file(path: &str) -> bool {
        path.ends_with(".ts")
            || path.ends_with(".tsx")
            || path.ends_with(".d.ts")
            || path.ends_with(".mts")
            || path.ends_with(".cts")
            || path.ends_with(".d.mts")
            || path.ends_with(".d.cts")
            || path.ends_with(".js")
            || path.ends_with(".jsx")
            || path.ends_with(".mjs")
            || path.ends_with(".cjs")
            || path.ends_with("/package.json")
            || path.ends_with("/tsconfig.json")
            || path.ends_with("/jsconfig.json")
    }

    fn sort_tsserver_completion_items(items: &mut [CompletionItem]) {
        fn compare_case_sensitive_ui(a: &str, b: &str) -> Ordering {
            fn split_numeric_segments(s: &str) -> Vec<&str> {
                let mut segments = Vec::new();
                let mut start = 0;
                let mut in_digit = false;

                for (i, ch) in s.char_indices() {
                    let is_digit = ch.is_ascii_digit();
                    if i == 0 {
                        in_digit = is_digit;
                    } else if is_digit != in_digit {
                        segments.push(&s[start..i]);
                        start = i;
                        in_digit = is_digit;
                    }
                }
                if start < s.len() {
                    segments.push(&s[start..]);
                }
                segments
            }

            let a_segments = split_numeric_segments(a);
            let b_segments = split_numeric_segments(b);

            for (a_seg, b_seg) in a_segments.iter().zip(b_segments.iter()) {
                let a_is_digit = a_seg.chars().next().is_some_and(|c| c.is_ascii_digit());
                let b_is_digit = b_seg.chars().next().is_some_and(|c| c.is_ascii_digit());

                let cmp = if a_is_digit && b_is_digit {
                    let a_num = a_seg.parse::<u64>().unwrap_or(0);
                    let b_num = b_seg.parse::<u64>().unwrap_or(0);
                    a_num.cmp(&b_num)
                } else {
                    a_seg.to_lowercase().cmp(&b_seg.to_lowercase())
                };

                if cmp != Ordering::Equal {
                    return cmp;
                }
            }

            let seg_cmp = a_segments.len().cmp(&b_segments.len());
            if seg_cmp != Ordering::Equal {
                return seg_cmp;
            }

            for (a_ch, b_ch) in a.chars().zip(b.chars()) {
                if a_ch == b_ch {
                    continue;
                }

                let a_lower = a_ch.to_lowercase().next().unwrap_or(a_ch);
                let b_lower = b_ch.to_lowercase().next().unwrap_or(b_ch);

                if a_lower == b_lower {
                    if a_ch.is_lowercase() && b_ch.is_uppercase() {
                        return Ordering::Less;
                    }
                    if a_ch.is_uppercase() && b_ch.is_lowercase() {
                        return Ordering::Greater;
                    }
                }
            }

            a.cmp(b)
        }

        fn compare_completion_sources(a: Option<&str>, b: Option<&str>) -> Ordering {
            match (a, b) {
                (Some(a), Some(b)) => {
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
                        .then_with(|| compare_case_sensitive_ui(a, b))
                }
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
            }
        }

        let compare_auto_import_variant_order = |a: &CompletionItem, b: &CompletionItem| {
            if a.label != b.label || a.source != b.source || !a.has_action || !b.has_action {
                return Ordering::Equal;
            }
            let a_export = Self::auto_import_export_name(a);
            let b_export = Self::auto_import_export_name(b);
            match (a_export.as_deref(), b_export.as_deref()) {
                (Some("default"), Some(other)) if other != "default" => Ordering::Less,
                (Some(other), Some("default")) if other != "default" => Ordering::Greater,
                _ => Ordering::Equal,
            }
        };

        items.sort_by(|a, b| {
            compare_case_sensitive_ui(a.effective_sort_text(), b.effective_sort_text())
                .then_with(|| compare_case_sensitive_ui(&a.label, &b.label))
                .then_with(|| compare_completion_sources(a.source.as_deref(), b.source.as_deref()))
                .then_with(|| compare_auto_import_variant_order(a, b))
        });
    }

    fn completion_entry_from_item(
        &self,
        current_file: &str,
        item: &tsz::lsp::completions::CompletionItem,
        line_map: &LineMap,
        source_text: &str,
        include_insert_text: bool,
    ) -> serde_json::Value {
        let effective_kind = self
            .auto_import_entry_kind_override(current_file, item)
            .unwrap_or_else(|| {
                if item.kind == CompletionItemKind::Variable
                    && Self::is_default_auto_import_item(item)
                {
                    CompletionItemKind::Property
                } else {
                    item.kind
                }
            });
        let kind = Self::completion_kind_to_str(effective_kind);
        let sort_text = item.effective_sort_text();
        let mut entry = serde_json::json!({
            "name": item.label,
            "kind": kind,
            "sortText": sort_text,
            "kindModifiers": item.kind_modifiers.clone().unwrap_or_default(),
        });

        let is_class_member_snippet = item.source.as_deref() == Some("ClassMemberSnippet/");
        if include_insert_text
            && let Some(insert_text) = item.insert_text.clone().or_else(|| {
                is_class_member_snippet
                    .then(|| Self::class_member_snippet_insert_text(item))
                    .flatten()
            })
        {
            let should_emit_insert_text =
                Self::should_emit_tsserver_insert_text(item, &insert_text, is_class_member_snippet);
            if should_emit_insert_text {
                entry["insertText"] = serde_json::json!(insert_text);
            }
        }
        if item.has_action {
            entry["hasAction"] = serde_json::json!(true);
            if item.is_snippet {
                entry["filterText"] = serde_json::json!(item.label.clone());
                if !is_class_member_snippet {
                    entry["isSnippet"] = serde_json::json!(true);
                }
            }
        }
        if item.is_package_json_import == Some(true) {
            entry["isPackageJsonImport"] = serde_json::json!(true);
        }
        if let Some(source) = item.source.as_ref() {
            entry["source"] = serde_json::json!(source);
            entry["sourceDisplay"] = serde_json::json!([{ "text": source, "kind": "text" }]);
            let mut data = serde_json::Map::new();
            data.insert("name".to_string(), serde_json::json!(item.label.clone()));
            data.insert("source".to_string(), serde_json::json!(source));
            if item.has_action {
                data.insert("moduleSpecifier".to_string(), serde_json::json!(source));
                if let Some(export_name) = Self::auto_import_export_name(item) {
                    data.insert("exportName".to_string(), serde_json::json!(export_name));
                    // Force worker-mode completion detail requests to stay on tsz for
                    // auto-import entries. Native fallback details can drop/reshape
                    // tags and action metadata for these entries.
                    data.insert(
                        "exportMapKey".to_string(),
                        serde_json::json!(format!("tsz::{source}::{}::{export_name}", item.label)),
                    );
                }
            }
            entry["data"] = serde_json::Value::Object(data);
        }
        if let Some((start, end)) = item.replacement_span {
            let start_pos = line_map.offset_to_position(start, source_text);
            let end_pos = line_map.offset_to_position(end, source_text);
            entry["replacementSpan"] = serde_json::json!({
                "start": Self::lsp_to_tsserver_position(start_pos),
                "end": Self::lsp_to_tsserver_position(end_pos),
            });
        }
        if item.label.starts_with('"') && item.label.ends_with('"') {
            entry["defaultCommitCharacters"] = serde_json::json!([",", "."]);
        }

        entry
    }

    fn should_emit_tsserver_insert_text(
        item: &CompletionItem,
        insert_text: &str,
        is_class_member_snippet: bool,
    ) -> bool {
        if insert_text.is_empty() {
            return false;
        }
        if is_class_member_snippet || item.has_action || !Self::is_identifier(&item.label) {
            return true;
        }
        if Self::is_plain_callable_snippet_insert_text(item, insert_text) {
            return false;
        }
        item.is_snippet || insert_text != item.label
    }

    fn is_plain_callable_snippet_insert_text(item: &CompletionItem, insert_text: &str) -> bool {
        matches!(
            item.kind,
            CompletionItemKind::Function
                | CompletionItemKind::Method
                | CompletionItemKind::Constructor
        ) && insert_text == format!("{}($1)", item.label)
    }

    fn last_optional_chain_token_start(source_text: &str, offset: u32) -> Option<u32> {
        let end = (offset as usize).min(source_text.len());
        source_text[..end].rfind("?.").map(|idx| idx as u32)
    }

    fn quoted_property_name_replacement_span(source_text: &str, offset: u32) -> Option<(u32, u32)> {
        let i = (offset as usize).min(source_text.len());
        let bytes = source_text.as_bytes();

        let mut quote_start = None;
        let mut j = i;
        while j > 0 {
            j -= 1;
            let b = bytes[j];
            if b == b'\n' || b == b'\r' {
                break;
            }
            if b == b'"' || b == b'\'' {
                quote_start = Some((j, b));
                break;
            }
        }
        let (start, quote) = quote_start?;
        let mut end = i;
        while end < bytes.len() {
            let b = bytes[end];
            if b == quote {
                break;
            }
            if b == b'\n' || b == b'\r' {
                return None;
            }
            end += 1;
        }
        if end >= bytes.len() || bytes[end] != quote {
            return None;
        }
        let mut k = end + 1;
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            if bytes[k] == b'\n' || bytes[k] == b'\r' {
                return None;
            }
            k += 1;
        }
        if k >= bytes.len() || bytes[k] != b':' {
            return None;
        }
        Some(((start + 1) as u32, end as u32))
    }

    fn is_line_comment_position(source_text: &str, offset: u32) -> bool {
        let i = (offset as usize).min(source_text.len());
        let line_start = source_text[..i].rfind('\n').map_or(0, |p| p + 1);
        source_text[line_start..i].contains("//")
    }

    fn is_import_meta_member_context(source_text: &str, offset: u32) -> bool {
        let end = (offset as usize).min(source_text.len());
        let trimmed = source_text[..end].trim_end();
        trimmed.ends_with("import.meta.") || trimmed.ends_with("import.meta")
    }

    fn extract_import_meta_members(source_text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut search_start = 0usize;
        while let Some(interface_idx) = source_text[search_start..].find("interface ImportMeta") {
            let abs = search_start + interface_idx;
            let Some(open_rel) = source_text[abs..].find('{') else {
                break;
            };
            let mut i = abs + open_rel + 1;
            let bytes = source_text.as_bytes();
            let mut depth = 1i32;
            let block_start = i;
            while i < source_text.len() && depth > 0 {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            if depth != 0 || i <= block_start {
                break;
            }
            let body = &source_text[block_start..i - 1];
            for line in body.lines() {
                let trimmed = line.trim_start();
                if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
                    continue;
                }
                let mut chars = trimmed.chars();
                let Some(first) = chars.next() else {
                    continue;
                };
                if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
                    continue;
                }
                let mut name = String::new();
                name.push(first);
                for ch in chars {
                    if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                        name.push(ch);
                    } else {
                        break;
                    }
                }
                if name.is_empty() {
                    continue;
                }
                let after_name = &trimmed[name.len()..].trim_start();
                if after_name.starts_with(':') || after_name.starts_with('(') {
                    out.push(name);
                }
            }
            search_start = i;
        }
        out
    }

    fn import_meta_project_completion_items(&self, file_name: &str) -> Vec<CompletionItem> {
        let mut out = Vec::new();
        let mut seen = FxHashSet::default();
        let scan_paths =
            Self::fallback_class_member_scan_paths(&self.open_files, &self.external_project_files);
        for path in scan_paths {
            if path == file_name {
                continue;
            }
            let Some(content) = self
                .open_files
                .get(&path)
                .cloned()
                .or_else(|| std::fs::read_to_string(&path).ok())
            else {
                continue;
            };
            for name in Self::extract_import_meta_members(&content) {
                if !seen.insert(name.clone()) {
                    continue;
                }
                let mut item = CompletionItem::new(name, CompletionItemKind::Property);
                item.sort_text = Some(sort_priority::MEMBER.to_string());
                out.push(item);
            }
        }
        out.sort_by(|a, b| a.label.cmp(&b.label));
        out
    }

    pub(crate) fn handle_completions(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let is_legacy_completions = request.command == "completions";
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            if let Some(base_offset) = line_map.position_to_offset(position, &source_text)
                && Self::is_line_comment_position(&source_text, base_offset)
            {
                if is_legacy_completions {
                    return Some(serde_json::json!([]));
                }
                return Some(serde_json::json!({
                    "isGlobalCompletion": false,
                    "isMemberCompletion": false,
                    "isNewIdentifierLocation": false,
                    "entries": []
                }));
            }
            let preferences = request
                .arguments
                .get("preferences")
                .unwrap_or(&request.arguments);
            let interner = TypeInterner::new();
            let provider = Completions::new_with_types(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.clone(),
            );
            let (completion_position, completion_result) =
                Self::completion_result_at_position(&provider, root, position);
            let provider_items = completion_result
                .as_ref()
                .map(|result| result.entries.clone())
                .unwrap_or_default();
            let project_completion_position = completion_position;
            let project_items = self.project_completion_items(
                &file,
                project_completion_position,
                Some(preferences),
            );
            let is_member_completion = completion_result
                .as_ref()
                .is_some_and(|result| result.is_member_completion);
            let allow_class_member_snippets = !is_member_completion
                && Self::is_class_member_snippet_context(
                    &source_text,
                    &line_map,
                    completion_position,
                );
            let include_class_member_snippets = Self::bool_pref_or_default(
                Some(preferences),
                "includeCompletionsWithClassMemberSnippets",
                self.include_completions_with_class_member_snippets,
            );
            let snippet_items = if include_class_member_snippets && allow_class_member_snippets {
                self.class_member_snippet_items(
                    &provider,
                    root,
                    completion_position,
                    &file,
                    &source_text,
                    &project_items,
                )
            } else {
                Vec::new()
            };
            let items = if is_member_completion {
                provider_items
            } else {
                Self::merge_non_member_completion_items(provider_items, project_items)
            };
            let mut items = items;
            if !snippet_items.is_empty() {
                items = Self::merge_non_member_completion_items(items, snippet_items);
                items = Self::prioritize_class_member_snippet_items(items);
                items = Self::normalize_class_member_snippet_items(items);
            }
            Self::sort_tsserver_completion_items(&mut items);
            let items = Self::prune_deeper_auto_import_duplicates(items);
            let mut items =
                self.maybe_add_verbatim_commonjs_auto_import_items(&file, &source_text, items);
            if let Some(completion_offset) =
                line_map.position_to_offset(completion_position, &source_text)
            {
                items = Self::maybe_add_merged_class_function_members(
                    items,
                    &source_text,
                    completion_offset,
                    is_member_completion,
                );
            }
            if is_member_completion
                && items.is_empty()
                && let Some(completion_offset) =
                    line_map.position_to_offset(completion_position, &source_text)
            {
                let fallback = self.commonjs_require_member_completion_items(
                    &file,
                    &source_text,
                    completion_offset,
                );
                if !fallback.is_empty() {
                    items = Self::merge_non_member_completion_items(items, fallback);
                }
            }
            Self::sort_tsserver_completion_items(&mut items);
            let items = Self::prune_deeper_auto_import_duplicates(items);
            let mut items = items;
            if is_member_completion
                && let Some(completion_offset) =
                    line_map.position_to_offset(completion_position, &source_text)
                && let Some(replacement_start) =
                    Self::last_optional_chain_token_start(&source_text, completion_offset)
            {
                for item in &mut items {
                    if item.replacement_span.is_none()
                        && item
                            .insert_text
                            .as_deref()
                            .is_some_and(|text| text.starts_with("?."))
                    {
                        item.replacement_span = Some((replacement_start, completion_offset));
                    }
                }
            }
            if !is_member_completion
                && let Some(completion_offset) =
                    line_map.position_to_offset(completion_position, &source_text)
            {
                if let Some((replacement_start, replacement_end)) =
                    Self::quoted_property_name_replacement_span(&source_text, completion_offset)
                {
                    for item in &mut items {
                        if item.replacement_span.is_none() {
                            item.replacement_span = Some((replacement_start, replacement_end));
                        }
                    }
                }
                let blocked = trailing_function_parameter_names_at_declaration_end(
                    &source_text,
                    completion_offset,
                );
                if !blocked.is_empty() {
                    items.retain(|item| !blocked.contains(&item.label));
                }
            }
            if is_member_completion
                && let Some(completion_offset) =
                    line_map.position_to_offset(completion_position, &source_text)
                && Self::is_import_meta_member_context(&source_text, completion_offset)
            {
                let project_meta_items = self.import_meta_project_completion_items(&file);
                if !project_meta_items.is_empty() {
                    items = Self::merge_non_member_completion_items(items, project_meta_items);
                    Self::sort_tsserver_completion_items(&mut items);
                }
            }
            let include_insert_text = Self::bool_pref_or_default(
                Some(preferences),
                "includeCompletionsWithInsertText",
                true,
            );

            let entries: Vec<serde_json::Value> = items
                .iter()
                .map(|item| {
                    self.completion_entry_from_item(
                        &file,
                        item,
                        &line_map,
                        &source_text,
                        include_insert_text,
                    )
                })
                .collect();
            let has_class_member_snippet = items
                .iter()
                .any(|item| item.source.as_deref() == Some("ClassMemberSnippet/"));
            let is_new_identifier_location = if Self::is_type_annotation_identifier_prefix_context(
                &source_text,
                &line_map,
                completion_position,
            ) {
                false
            } else if (include_class_member_snippets && has_class_member_snippet)
                || Self::is_class_member_declaration_prefix_context(
                    &source_text,
                    &line_map,
                    completion_position,
                )
            {
                true
            } else if Self::is_bare_identifier_expression_prefix(
                &source_text,
                &line_map,
                completion_position,
            ) {
                false
            } else {
                completion_result
                    .as_ref()
                    .is_some_and(|r| r.is_new_identifier_location)
            };
            let default_commit_characters =
                (!is_new_identifier_location).then_some(serde_json::json!([".", ",", ";"]));

            if is_legacy_completions {
                return Some(serde_json::Value::Array(entries));
            }

            let mut response = serde_json::json!({
                "isGlobalCompletion": completion_result.as_ref().is_some_and(|r| r.is_global_completion),
                "isMemberCompletion": completion_result.as_ref().is_some_and(|r| r.is_member_completion),
                "isNewIdentifierLocation": is_new_identifier_location,
                "entries": entries,
            });
            if let Some(default_commit_characters) = default_commit_characters {
                response["defaultCommitCharacters"] = default_commit_characters;
            }

            Some(response)
        })();
        let fallback = if is_legacy_completions {
            serde_json::json!([])
        } else {
            serde_json::json!({
                "isGlobalCompletion": false,
                "isMemberCompletion": false,
                "isNewIdentifierLocation": false,
                "entries": []
            })
        };
        self.success_response(seq, request, Some(result.unwrap_or(fallback)))
    }

    // Display parts rendering, signature help handler, and tokenization utilities
    // are in handlers_completions_display.rs
}
