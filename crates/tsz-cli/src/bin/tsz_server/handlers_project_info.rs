//! Project-info helpers for tsz-server.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::emitter::ScriptTarget;

/// `projectInfo`-only view of inferred-project lib/target/noLib settings.
/// Kept parallel to `Server.inferred_check_options` so we can surface the
/// right lib list from `getProjectInfo` without changing the lib set seen
/// by `check.rs` (which drives every typecheck and is perf-sensitive).
#[derive(Debug, Clone, Default)]
pub(crate) struct InferredProjectInfoOptions {
    pub lib: Option<Vec<String>>,
    pub target: Option<String>,
    pub no_lib: bool,
}

impl Server {
    pub(super) fn apply_inferred_project_options(&mut self, options: Option<&serde_json::Value>) {
        if let Some(options) = options {
            self.inferred_check_options =
                serde_json::from_value(options.clone()).unwrap_or_default();
            // Stash a projectInfo-only view of the raw lib/target/noLib
            // payload. Writing these onto `inferred_check_options` would pull
            // in lib loading on every rename/completion typecheck (measurable
            // slowdown across rename* fourslash tests); this parallel state
            // only feeds `handle_project_info`.
            let mut libs: Option<Vec<String>> = None;
            if let Some(arr) = options.get("lib").and_then(serde_json::Value::as_array) {
                let parsed: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                    .collect();
                if !parsed.is_empty() {
                    libs = Some(parsed);
                }
            }
            let target = Self::project_info_target_option(options.get("target"));
            let no_lib = options
                .get("noLib")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            self.inferred_projectinfo_options = Some(InferredProjectInfoOptions {
                lib: libs,
                target,
                no_lib,
            });
            self.allow_importing_ts_extensions = options
                .get("allowImportingTsExtensions")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            self.inferred_module_is_none_for_projects = options
                .get("module")
                .is_some_and(Self::inferred_module_option_is_none);
            self.auto_imports_allowed_for_inferred_projects =
                Self::inferred_auto_imports_allowed(options);
        }
    }

    fn project_info_target_option(value: Option<&serde_json::Value>) -> Option<String> {
        match value? {
            serde_json::Value::String(target) => Some(target.clone()),
            serde_json::Value::Number(number) => {
                let mapped = number
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .and_then(ScriptTarget::from_ts_numeric)
                    .map(ScriptTarget::as_ts_str);
                Some(mapped.map_or_else(|| number.to_string(), str::to_string))
            }
            _ => None,
        }
    }

    pub(crate) fn inferred_auto_imports_allowed(options: &serde_json::Value) -> bool {
        let module_none = options
            .get("module")
            .is_some_and(Self::inferred_module_option_is_none);
        if !module_none {
            return true;
        }

        options
            .get("target")
            .is_some_and(Self::inferred_target_supports_import_syntax)
    }

    fn inferred_module_option_is_none(value: &serde_json::Value) -> bool {
        if let Some(v) = value.as_str() {
            return v.eq_ignore_ascii_case("none") || v.parse::<i64>().ok() == Some(0);
        }
        value.as_i64() == Some(0)
    }

    fn inferred_target_supports_import_syntax(value: &serde_json::Value) -> bool {
        if let Some(target) = value.as_str() {
            if let Ok(numeric_target) = target.parse::<i64>() {
                return numeric_target >= 2;
            }

            return target.eq_ignore_ascii_case("es6")
                || target.eq_ignore_ascii_case("es2015")
                || target.eq_ignore_ascii_case("es2016")
                || target.eq_ignore_ascii_case("es2017")
                || target.eq_ignore_ascii_case("es2018")
                || target.eq_ignore_ascii_case("es2019")
                || target.eq_ignore_ascii_case("es2020")
                || target.eq_ignore_ascii_case("es2021")
                || target.eq_ignore_ascii_case("es2022")
                || target.eq_ignore_ascii_case("es2023")
                || target.eq_ignore_ascii_case("es2024")
                || target.eq_ignore_ascii_case("esnext")
                || target.eq_ignore_ascii_case("latest");
        }

        value.as_i64().is_some_and(|target| target >= 2)
    }

    pub(crate) fn handle_project_info(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let need_file_name_list = request
                .arguments
                .get("needFileNameList")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let (config_file_name, file_names) = self.compute_project_info(file);
            let mut response = serde_json::json!({
                "configFileName": config_file_name,
            });
            if need_file_name_list {
                response["fileNames"] = serde_json::json!(file_names);
            }
            Some(response)
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"configFileName": "", "fileNames": []}))),
        )
    }

    /// Virtual lib folder used by the fourslash harness. Matches TypeScript's
    /// `vfsUtil.ts:fourslashLibFolder` so that the expected file paths returned
    /// by `getProjectInfo` line up with the harness's VFS mount.
    const FOURSLASH_LIB_FOLDER: &'static str = "/home/src/tslibs/TS/Lib";

    /// Compute the (configFileName, fileNames) tuple for a `projectInfo` request.
    ///
    /// Order of `fileNames`:
    ///   1. Resolved lib files (default or explicit) anchored at `FOURSLASH_LIB_FOLDER`.
    ///   2. Project files (from tsconfig `files`, or active+transitive deps if inferred).
    ///   3. The tsconfig/jsconfig file itself, if one was found.
    ///
    /// Project files are filtered to those present in `open_files` (virtual VFS
    /// membership) to match tsserver's behavior of excluding non-existent files.
    pub(crate) fn compute_project_info(&self, active_file: &str) -> (String, Vec<String>) {
        if let Some(project_info) = self.external_project_info(active_file) {
            return project_info;
        }

        let config_file_name = self.find_project_config_file(active_file);
        let (lib_names, no_lib, project_files) = match config_file_name.as_deref() {
            Some(config_path) => self.parse_tsconfig_for_project_info(config_path),
            None => self.inferred_project_info(active_file),
        };

        let mut file_names: Vec<String> = Vec::new();

        if !no_lib {
            for path in self.resolve_virtual_lib_files(&lib_names, Some(active_file)) {
                file_names.push(path);
            }
        }

        for project_file in project_files {
            file_names.push(project_file);
        }

        if let Some(ref config_path) = config_file_name {
            file_names.push(config_path.clone());
        }

        (config_file_name.unwrap_or_default(), file_names)
    }

    fn external_project_info(&self, active_file: &str) -> Option<(String, Vec<String>)> {
        let mut projects: Vec<(&String, &Vec<String>)> =
            self.external_project_files.iter().collect();
        projects.sort_by_key(|(project_name, _)| *project_name);

        let (project_name, files) = projects
            .into_iter()
            .find(|(_, files)| files.iter().any(|file| file == active_file))?;
        let mut file_names = files.clone();
        file_names.sort();
        file_names.dedup();

        Some((project_name.clone(), file_names))
    }

    /// Parse a tsconfig (from `open_files` or disk) and return its lib list,
    /// `noLib` flag, and resolved project files (filtered by existence in VFS).
    pub(super) fn parse_tsconfig_for_project_info(
        &self,
        config_path: &str,
    ) -> (Vec<String>, bool, Vec<String>) {
        let Some(config_json) = self.read_config_json(config_path) else {
            return (Self::default_lib_names_for_target(None), false, Vec::new());
        };
        let compiler_options = config_json.get("compilerOptions");
        let no_lib = compiler_options
            .and_then(|opts| opts.get("noLib"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let lib_list = compiler_options
            .and_then(|opts| opts.get("lib"))
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            });
        let target = compiler_options
            .and_then(|opts| opts.get("target"))
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string());
        let lib_names =
            lib_list.unwrap_or_else(|| Self::default_lib_names_for_target(target.as_deref()));

        let config_dir = std::path::Path::new(config_path)
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("/"));

        let project_files = self.tsconfig_project_files(&config_json, &config_dir);

        (lib_names, no_lib, project_files)
    }

    fn tsconfig_project_files(
        &self,
        config_json: &serde_json::Value,
        config_dir: &std::path::Path,
    ) -> Vec<String> {
        if let Some(files) = config_json
            .get("files")
            .and_then(serde_json::Value::as_array)
        {
            return self.explicit_tsconfig_files(files, config_dir);
        }

        let includes = config_json
            .get("include")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
            })
            .filter(|arr| !arr.is_empty())
            .unwrap_or_else(|| vec!["**/*"]);
        let excludes = config_json
            .get("exclude")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["node_modules", "bower_components", "jspm_packages"]);

        let Some(include_set) = Self::tsconfig_glob_set(&includes, true) else {
            return Vec::new();
        };
        let exclude_set = Self::tsconfig_glob_set(&excludes, false);

        let mut candidates: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for file in self.open_files.keys() {
            if Self::path_is_under(file, config_dir) && Self::is_supported_project_source_file(file)
            {
                candidates.insert(file.clone());
            }
        }

        if config_dir.exists() {
            for entry in walkdir::WalkDir::new(config_dir)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
            {
                let normalized = Self::normalize_path_string(entry.path());
                if Self::is_supported_project_source_file(&normalized) {
                    candidates.insert(normalized);
                }
            }
        }

        candidates
            .into_iter()
            .filter(|path| {
                let relative = Self::relative_slash_path(config_dir, path);
                include_set.is_match(&relative)
                    && !exclude_set
                        .as_ref()
                        .is_some_and(|exclude_set| exclude_set.is_match(&relative))
            })
            .collect()
    }

    fn explicit_tsconfig_files(
        &self,
        files: &[serde_json::Value],
        config_dir: &std::path::Path,
    ) -> Vec<String> {
        let mut project_files = Vec::new();
        for entry in files {
            let Some(name) = entry.as_str() else {
                continue;
            };
            let absolute = if std::path::Path::new(name).is_absolute() {
                std::path::PathBuf::from(name)
            } else {
                config_dir.join(name)
            };
            let absolute_str = Self::normalize_path_string(&absolute);
            // tsserver excludes files that don't physically exist.
            if self.open_files.contains_key(&absolute_str)
                || std::path::Path::new(&absolute_str).exists()
            {
                project_files.push(absolute_str);
            }
        }
        project_files
    }

    fn tsconfig_glob_set(patterns: &[&str], include: bool) -> Option<globset::GlobSet> {
        let mut builder = globset::GlobSetBuilder::new();
        for pattern in patterns {
            let normalized = pattern.trim().trim_start_matches("./").replace('\\', "/");
            if normalized.is_empty() {
                continue;
            }
            let has_wildcard = normalized.contains('*') || normalized.contains('?');
            let expanded = if has_wildcard {
                normalized
            } else if include || !Self::is_supported_project_source_file(&normalized) {
                format!("{normalized}/**/*")
            } else {
                normalized
            };
            let glob = globset::GlobBuilder::new(&expanded)
                .literal_separator(true)
                .build()
                .ok()?;
            builder.add(glob);
        }
        builder.build().ok()
    }

    fn relative_slash_path(config_dir: &std::path::Path, file: &str) -> String {
        let file_path = std::path::Path::new(file);
        file_path
            .strip_prefix(config_dir)
            .map(Self::normalize_path_string)
            .unwrap_or_else(|_| file.trim_start_matches('/').to_string())
    }

    fn path_is_under(path: &str, dir: &std::path::Path) -> bool {
        std::path::Path::new(path).starts_with(dir)
    }

    pub(super) fn is_supported_project_source_file(path: &str) -> bool {
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
    }

    /// For an inferred project (no tsconfig), produce (libs, noLib, [`active+transitive_deps`]).
    pub(super) fn inferred_project_info(
        &self,
        active_file: &str,
    ) -> (Vec<String>, bool, Vec<String>) {
        // Prefer the projectInfo-only view populated by
        // `apply_inferred_project_options` — it carries the raw lib/target/noLib
        // from the `compilerOptionsForInferredProjects` call. When that mirror
        // state is absent (e.g. direct test setup), fall back to
        // `inferred_check_options` so `noLib` and lib/target overrides still
        // affect projectInfo consistently.
        let (lib_override, target_override, no_lib): (Option<Vec<String>>, Option<String>, bool) =
            match self.inferred_projectinfo_options.as_ref() {
                Some(opts) => (opts.lib.clone(), opts.target.clone(), opts.no_lib),
                None => (
                    self.inferred_check_options.lib.clone(),
                    self.inferred_check_options.target.clone(),
                    self.inferred_check_options.no_lib,
                ),
            };
        let lib_names = lib_override
            .unwrap_or_else(|| Self::default_lib_names_for_target(target_override.as_deref()));

        let mut project_files: Vec<String> = Vec::new();
        if self.open_files.contains_key(active_file) {
            let mut visited: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
            self.collect_reachable_files(active_file, &mut visited, &mut project_files);
        }

        (lib_names, no_lib, project_files)
    }

    /// Walk triple-slash path references and relative import specifiers from
    /// `file`, adding each file that exists in `open_files` to `out` in
    /// depth-first order (dependencies before the referencing file).
    pub(super) fn collect_reachable_files(
        &self,
        file: &str,
        visited: &mut rustc_hash::FxHashSet<String>,
        out: &mut Vec<String>,
    ) {
        if !visited.insert(file.to_string()) {
            return;
        }
        let Some(content) = self.open_files.get(file) else {
            return;
        };
        let parent = std::path::Path::new(file)
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        for specifier in Self::extract_reference_specifiers(content) {
            for candidate in Self::resolve_relative_specifier(&parent, &specifier) {
                if self.open_files.contains_key(&candidate) {
                    self.collect_reachable_files(&candidate, visited, out);
                    break;
                }
            }
        }
        out.push(file.to_string());
    }

    /// Extract triple-slash path references and relative module specifiers
    /// from the top of a source file (before the first statement line).
    fn extract_reference_specifiers(source: &str) -> Vec<String> {
        let mut specs: Vec<String> = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim_start();
            if let Some(path) = Self::parse_triple_slash_path(trimmed) {
                specs.push(path);
                continue;
            }
            if let Some(spec) = Self::parse_import_or_export_specifier(trimmed) {
                if spec.starts_with("./") || spec.starts_with("../") {
                    specs.push(spec);
                }
                continue;
            }
        }
        specs
    }

    fn parse_triple_slash_path(line: &str) -> Option<String> {
        if !line.starts_with("///") {
            return None;
        }
        let idx = line.find("path=")?;
        let tail = &line[idx + "path=".len()..];
        let quote = tail.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let rest = &tail[1..];
        let end = rest.find(quote)?;
        Some(rest[..end].to_string())
    }

    fn parse_import_or_export_specifier(line: &str) -> Option<String> {
        if !(line.starts_with("import") || line.starts_with("export")) {
            return None;
        }
        // Find a quoted string as the specifier (avoid matching identifiers).
        let from_idx = line.find(" from ").or_else(|| line.find("\tfrom\t"));
        let search_start = match from_idx {
            Some(i) => i + 6,
            None => {
                // bare `import "x"` form
                if !line.starts_with("import") {
                    return None;
                }
                "import".len()
            }
        };
        let rest = line.get(search_start..)?;
        let trimmed = rest.trim_start();
        let quote = trimmed.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let inner = &trimmed[1..];
        let end = inner.find(quote)?;
        Some(inner[..end].to_string())
    }

    /// Resolve a relative module specifier (e.g. "./a", "./a.ts") against a
    /// parent directory. Generates candidate absolute paths with standard TS
    /// extensions, in priority order, so callers can probe `open_files`/disk.
    fn resolve_relative_specifier(parent: &std::path::Path, specifier: &str) -> Vec<String> {
        let candidate_exts = [".ts", ".tsx", ".d.ts", ".js", ".jsx", ".mts", ".cts", ""];
        let mut candidates = Vec::with_capacity(candidate_exts.len());
        for ext in candidate_exts {
            let candidate = parent.join(format!("{specifier}{ext}"));
            let normalized = Self::normalize_path_string(&candidate);
            if !normalized.is_empty() {
                candidates.push(normalized);
            }
        }
        candidates
    }

    pub(super) fn normalize_path_string(path: &std::path::Path) -> String {
        // Normalize `.` and `..` components without touching the filesystem.
        let mut components: Vec<String> = Vec::new();
        let mut absolute = false;
        for comp in path.components() {
            use std::path::Component;
            match comp {
                Component::RootDir => {
                    absolute = true;
                    components.clear();
                }
                Component::Prefix(_) | Component::CurDir => {}
                Component::ParentDir => {
                    components.pop();
                }
                Component::Normal(name) => {
                    components.push(name.to_string_lossy().to_string());
                }
            }
        }
        let joined = components.join("/");
        if absolute {
            format!("/{joined}")
        } else {
            joined
        }
    }

    /// Read tsconfig JSON from `open_files` (VFS) first, then fall back to disk.
    /// Strips JSONC comments/trailing commas so tsconfig sources that aren't
    /// strict JSON still parse.
    pub(super) fn read_config_json(&self, config_path: &str) -> Option<serde_json::Value> {
        use tsz_cli::config::strip_jsonc;
        let content = self
            .open_files
            .get(config_path)
            .cloned()
            .or_else(|| std::fs::read_to_string(config_path).ok())?;
        let stripped = strip_jsonc(&content);
        serde_json::from_str::<serde_json::Value>(&stripped)
            .ok()
            .or_else(|| serde_json::from_str::<serde_json::Value>(&content).ok())
    }

    /// Find the nearest tsconfig/jsconfig for a file. Checks `open_files` (VFS)
    /// before the disk to support fourslash virtual filesystems.
    pub(super) fn find_project_config_file(&self, file: &str) -> Option<String> {
        let mut current = std::path::Path::new(file).parent();
        while let Some(dir) = current {
            for name in ["tsconfig.json", "jsconfig.json"] {
                let config_path = dir.join(name);
                let as_string = Self::normalize_path_string(&config_path);
                if self.open_files.contains_key(&as_string) {
                    return Some(as_string);
                }
                if config_path.exists() {
                    return Some(config_path.to_string_lossy().to_string());
                }
            }
            current = dir.parent();
        }
        None
    }

    fn default_lib_names_for_target(target: Option<&str>) -> Vec<String> {
        use tsz_cli::config::default_lib_name_for_target;
        let emitter_target = super::Server::parse_target(&target.map(str::to_string));
        vec![default_lib_name_for_target(emitter_target).to_string()]
    }

    /// Resolve lib names to file paths, branching on whether the originating
    /// file is part of the fourslash harness's virtual filesystem.
    ///
    /// - Fourslash paths (e.g. `/tests/cases/fourslash/foo.ts`): rewrite to
    ///   the harness's virtual lib folder so expected paths line up with the
    ///   VFS mount (`/home/src/tslibs/TS/Lib/lib.es5.d.ts`).
    /// - Real on-disk paths: return the actual lib file paths the server is
    ///   using, matching tsserver's `projectInfo` protocol behavior.
    pub(super) fn resolve_virtual_lib_files(
        &self,
        lib_names: &[String],
        active_file: Option<&str>,
    ) -> Vec<String> {
        use tsz_cli::config::resolve_lib_files_from_dir;
        if lib_names.is_empty() {
            return Vec::new();
        }
        let use_virtual = active_file
            .map(Self::is_fourslash_virtual_harness_path)
            .unwrap_or(false);
        let lib_dirs = [self.lib_dir.as_path(), self.tests_lib_dir.as_path()];
        let mut last_err: Option<anyhow::Error> = None;
        for dir in lib_dirs {
            match resolve_lib_files_from_dir(lib_names, dir) {
                Ok(paths) => {
                    return paths
                        .into_iter()
                        .filter_map(|p| {
                            if use_virtual {
                                p.file_name().and_then(|s| s.to_str()).map(|name| {
                                    // tsz stores libs without the `lib.` prefix in some
                                    // layouts (source tree). Ensure the emitted name
                                    // matches tsc's runtime convention `lib.<id>.d.ts`.
                                    let out = if name.starts_with("lib.") {
                                        name.to_string()
                                    } else {
                                        format!("lib.{name}")
                                    };
                                    format!("{}/{}", Self::FOURSLASH_LIB_FOLDER, out)
                                })
                            } else {
                                Some(p.to_string_lossy().into_owned())
                            }
                        })
                        .collect();
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }
        if let Some(err) = last_err {
            tracing::debug!("resolve_virtual_lib_files failed: {err}");
        }
        Vec::new()
    }
}
