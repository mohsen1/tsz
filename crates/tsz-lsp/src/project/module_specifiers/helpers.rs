use super::*;
use tsz_common::file_extensions::{
    strip_known_extension, strip_ts_extension as strip_ts_specifier_extension,
};

/// Rank a [`SpecifierCandidateSet`] into a deduplicated, ordered list of
/// import specifier strings, applying the caller's `pref` policy.
///
/// This is a pure function: it has no access to project state and does not
/// perform any further path construction. All policy lives here; collection
/// lives in [`Project::collect_specifier_candidates`].
pub(super) fn rank_specifier_candidates(
    candidates: SpecifierCandidateSet,
    pref: Option<ImportSpecifierPreference>,
) -> Vec<String> {
    let SpecifierCandidateSet {
        relative,
        workspace_package,
        node_modules_package,
        target_in_node_modules,
    } = candidates;

    let Some(rel) = relative else {
        // No relative path reachable — return only package specifiers.
        let mut only_packages: Vec<String> = workspace_package.into_iter().collect();
        only_packages.extend(node_modules_package);
        dedup_in_place(&mut only_packages);
        return only_packages;
    };

    let RelativeCandidates {
        relative,
        root_dirs_relative,
        path_mappings,
        package_imports,
    } = rel;

    let mut ranked = Vec::new();

    match pref {
        Some(ImportSpecifierPreference::NonRelative) => {
            ranked.extend(path_mappings);
            ranked.extend(package_imports);
            ranked.extend(workspace_package);
            ranked.extend(node_modules_package);
            ranked.push(relative);
            ranked.extend(root_dirs_relative);
        }
        Some(ImportSpecifierPreference::Relative | ImportSpecifierPreference::ProjectRelative) => {
            // Both variants share identical candidate ordering; they differ only
            // in whether workspace fallback without a requesting package.json is
            // active (see `prefers_project_relative_workspace_fallback…`).
            //
            // Explicit path mappings (tsconfig `paths`) are user-declared and
            // take precedence over workspace-package specifiers. `package_imports`
            // (package.json `imports` / `#…` specifiers) must stay behind the
            // relative specifier.
            ranked.extend(path_mappings);
            // TypeScript still prefers declared dependency specifiers over deep
            // relative traversals, even under explicit `relative` preference.
            ranked.extend(workspace_package);
            ranked.extend(node_modules_package);
            ranked.push(relative);
            ranked.extend(root_dirs_relative);
            ranked.extend(package_imports);
        }
        None => {
            ranked.push(relative);
            ranked.extend(root_dirs_relative);
            ranked.extend(path_mappings);
            ranked.extend(package_imports);
            ranked.extend(workspace_package);
            ranked.extend(node_modules_package);
        }
    }

    dedup_in_place(&mut ranked);
    if target_in_node_modules {
        ranked.retain(|spec| !spec.replace('\\', "/").contains("node_modules/"));
    }

    match pref {
        None => ranked.sort_by(compare_module_specifier_candidates),
        Some(ImportSpecifierPreference::NonRelative) => {
            ranked.sort_by(|a, b| {
                let a_relative = a.starts_with('.');
                let b_relative = b.starts_with('.');
                a_relative
                    .cmp(&b_relative)
                    .then_with(|| compare_module_specifier_candidates(a, b))
            });
        }
        Some(ImportSpecifierPreference::Relative | ImportSpecifierPreference::ProjectRelative) => {}
    }

    ranked
}

pub(super) fn dedup_in_place(v: &mut Vec<String>) {
    if v.len() <= 2 {
        if v.len() == 2 && v[0] == v[1] {
            v.truncate(1);
        }
        return;
    }
    let mut seen = FxHashSet::default();
    v.retain(|s| seen.insert(s.clone()));
}

/// Lexically collapse `.`/`..` in a joined package/config-relative path.
///
/// Delegates to the canonical
/// [`tsz_common::module_resolution::path_identity::normalize_segments`]:
/// `..` clamps at the filesystem root (as the historical local loop already
/// did) and an unmatched `..` on a relative path is kept (the historical
/// loop dropped it; callers always join onto rooted package/config
/// directories, where the two behaviors agree).
pub(super) fn normalize_path(path: &Path) -> PathBuf {
    tsz_common::module_resolution::path_identity::normalize_segments(path)
}

fn strip_path_file_extension(path: &Path, strip: fn(&str) -> &str) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };

    let base_name = strip(file_name);
    if base_name == file_name || base_name.is_empty() {
        return path.to_path_buf();
    }

    let mut base = PathBuf::new();
    if let Some(parent) = path.parent() {
        base.push(parent);
    }
    base.push(base_name);
    base
}

pub(super) fn strip_ts_path_extension(path: &Path) -> PathBuf {
    strip_path_file_extension(path, strip_ts_specifier_extension)
}

pub(super) fn split_node_modules_package_path(package_path: &str) -> Option<(String, String)> {
    let mut segments = package_path.split('/');
    let first = segments.next()?;
    if first.is_empty() {
        return None;
    }

    if first.starts_with('@') {
        let second = segments.next()?;
        let package_root = format!("{first}/{second}");
        let suffix = segments.collect::<Vec<_>>().join("/");
        Some((package_root, suffix))
    } else {
        let suffix = segments.collect::<Vec<_>>().join("/");
        Some((first.to_string(), suffix))
    }
}

pub(super) fn normalize_node_modules_package_specifier(package_specifier: &str) -> String {
    let mut normalized = package_specifier.replace('\\', "/");
    if let Some(stripped) = normalized.strip_suffix("/index")
        && !stripped.is_empty()
    {
        normalized = stripped.to_string();
    }

    if let Some(stripped) = normalized.strip_prefix("@types/") {
        let mut parts = stripped.splitn(2, '/');
        let package_name = parts.next().unwrap_or_default();
        let rest = parts.next();

        let package_name = if let Some((scope, name)) = package_name.split_once("__") {
            format!("@{scope}/{name}")
        } else {
            package_name.to_string()
        };

        return match rest {
            Some(rest) if !rest.is_empty() && rest != "index" => {
                format!("{package_name}/{rest}")
            }
            _ => package_name,
        };
    }

    normalized
}

pub(super) fn normalize_path_mapping_specifier(specifier: &str) -> String {
    specifier
        .strip_suffix("/index")
        .unwrap_or(specifier)
        .to_string()
}

pub(super) fn package_runtime_specifier_from_target_path(package_path: &str) -> String {
    let normalized = package_path.replace('\\', "/");

    if let Some(base) = normalized.strip_suffix(".d.mts") {
        return format!("{base}.mjs");
    }
    if let Some(base) = normalized.strip_suffix(".d.cts") {
        return format!("{base}.cjs");
    }
    if let Some(base) = normalized.strip_suffix(".d.ts") {
        return base.to_string();
    }
    // For TS/TSX source files under node_modules (symlinked packages), the
    // runtime specifier is the extension-less form so downstream normalization
    // can collapse `pkg/index` to `pkg`.
    if let Some(base) = normalized.strip_suffix(".mts") {
        return format!("{base}.mjs");
    }
    if let Some(base) = normalized.strip_suffix(".cts") {
        return format!("{base}.cjs");
    }
    if let Some(base) = normalized
        .strip_suffix(".ts")
        .or_else(|| normalized.strip_suffix(".tsx"))
    {
        return base.to_string();
    }

    normalized
}

pub(super) fn is_declaration_source_path(path: &str) -> bool {
    path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts")
}

pub(super) fn normalize_package_entry_for_match(path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = path.strip_prefix("./").unwrap_or(&path);
    let stripped = path_to_string(&strip_js_ts_extension(Path::new(path))).replace('\\', "/");
    stripped
        .strip_suffix("/index")
        .unwrap_or(&stripped)
        .to_string()
}

pub(super) fn package_main_module_specifier_for_target(
    package_json: &serde_json::Value,
    package_root: &str,
    runtime_package_spec: &str,
    target_file: &str,
) -> Option<String> {
    let package_prefix = format!("{package_root}/");
    let runtime_subpath = runtime_package_spec.strip_prefix(&package_prefix)?;
    let runtime_normalized = normalize_package_entry_for_match(runtime_subpath);
    if runtime_normalized.is_empty() {
        return None;
    }

    let package_type_module = package_json
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value == "module");

    if is_declaration_source_path(target_file) {
        // For declaration targets, only treat package `types`/`typings` entries
        // as root aliases. Runtime `main`/`module` declarations should not
        // collapse arbitrary .d.ts subpaths to the package root.
        for entry_field in ["types", "typings"] {
            let Some(entry) = package_json
                .get(entry_field)
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let entry_normalized = normalize_package_entry_for_match(entry);
            if !entry_normalized.is_empty() && entry_normalized == runtime_normalized {
                return Some(package_root.to_string());
            }
        }
        return None;
    }

    for entry_field in ["module", "main"] {
        let Some(entry) = package_json
            .get(entry_field)
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let entry_normalized = normalize_package_entry_for_match(entry);
        if entry_normalized.is_empty() || entry_normalized != runtime_normalized {
            continue;
        }

        if package_type_module {
            return Some(format!("{package_root}/{entry_normalized}"));
        }

        return Some(package_root.to_string());
    }

    None
}

pub(super) fn has_ts_extension(module_text: &str) -> bool {
    module_text.ends_with(".ts")
        || module_text.ends_with(".tsx")
        || module_text.ends_with(".mts")
        || module_text.ends_with(".cts")
}

pub(super) fn has_js_extension(module_text: &str) -> bool {
    module_text.ends_with(".js")
        || module_text.ends_with(".jsx")
        || module_text.ends_with(".mjs")
        || module_text.ends_with(".cjs")
}

pub(super) fn ts_source_extension(target_file: &str) -> Option<&'static str> {
    if target_file.ends_with(".tsx") {
        Some(".tsx")
    } else if target_file.ends_with(".ts") && !target_file.ends_with(".d.ts") {
        Some(".ts")
    } else if target_file.ends_with(".mts") && !target_file.ends_with(".d.mts") {
        Some(".mts")
    } else if target_file.ends_with(".cts") && !target_file.ends_with(".d.cts") {
        Some(".cts")
    } else {
        None
    }
}

pub(super) fn target_supports_import_syntax(target: &str) -> bool {
    let target = target.trim();
    if let Ok(numeric_target) = target.parse::<i64>() {
        return numeric_target >= 2;
    }

    target.eq_ignore_ascii_case("es6")
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
        || target.eq_ignore_ascii_case("latest")
}

pub(super) fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from_components: Vec<_> = from
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect();
    let to_components: Vec<_> = to
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect();

    let mut common = 0;
    while common < from_components.len()
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }

    let mut result = PathBuf::new();
    for _ in common..from_components.len() {
        result.push("..");
    }
    for component in &to_components[common..] {
        result.push(component.as_os_str());
    }

    if result.as_os_str().is_empty() {
        result.push(".");
    }

    result
}

pub(super) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub(super) fn parse_typescript_config_json(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str(text)
        .ok()
        .or_else(|| json5::from_str::<serde_json::Value>(text).ok())
        .or_else(|| {
            // tsconfig.json is permissively parsed by TypeScript — missing
            // commas between members on separate lines are tolerated. Insert
            // a comma after `}` / `]` / scalar values when the next
            // non-whitespace character (after optional newline) is a
            // double-quoted key, and retry parsing.
            let repaired = repair_tsconfig_missing_commas(text);
            serde_json::from_str(&repaired)
                .ok()
                .or_else(|| json5::from_str::<serde_json::Value>(&repaired).ok())
        })
}

pub(super) fn repair_tsconfig_missing_commas(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + 16);
    let mut i = 0;
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_line_comment {
            out.push(c as char);
            if c == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            out.push(c as char);
            if c == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                out.push('/');
                i += 2;
                in_block_comment = false;
                continue;
            }
            i += 1;
            continue;
        }
        if in_string {
            out.push(c as char);
            if c == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            out.push('"');
            in_string = true;
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'/' => {
                    out.push_str("//");
                    in_line_comment = true;
                    i += 2;
                    continue;
                }
                b'*' => {
                    out.push_str("/*");
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        let needs_comma_after = matches!(c, b'}' | b']' | b'"' | b'0'..=b'9' | b'e' | b'l' | b'r');
        out.push(c as char);
        i += 1;
        if needs_comma_after {
            let mut j = i;
            let mut saw_newline = false;
            while j < bytes.len() {
                let nc = bytes[j];
                if nc == b'\n' {
                    saw_newline = true;
                    j += 1;
                } else if matches!(nc, b' ' | b'\t' | b'\r') {
                    j += 1;
                } else {
                    break;
                }
            }
            if saw_newline && j < bytes.len() && bytes[j] == b'"' {
                out.push(',');
            }
        }
    }
    out
}

pub(super) fn compare_module_specifier_candidates(a: &String, b: &String) -> Ordering {
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
    let a_rank = candidate_rank(a);
    let b_rank = candidate_rank(b);
    a_segments
        .cmp(&b_segments)
        .then_with(|| a_rank.cmp(&b_rank))
        .then_with(|| a.cmp(b))
}

#[cfg(test)]
pub(super) fn package_import_specifiers_for_target(
    package_json_text: &str,
    package_dir: &str,
    target_file: &str,
    allow_importing_ts_extensions: bool,
    additional_targets: &[String],
) -> Vec<String> {
    package_import_specifiers_for_target_detailed(
        package_json_text,
        package_dir,
        target_file,
        allow_importing_ts_extensions,
        additional_targets,
    )
    .into_iter()
    .map(|(specifier, _ambiguous)| specifier)
    .collect()
}

/// Same matching as [`package_import_specifiers_for_target`], but each
/// returned specifier is paired with whether it came from a *conditionally
/// ambiguous* `imports` entry: an object whose non-`types` condition
/// branches (`browser`/`default`/custom conditions/…) resolve to more than
/// one distinct physical file. A `#pattern` specifier drawn from such an
/// entry does not, on its own, deterministically address the matched file —
/// which condition set is active decides that — so callers use this flag to
/// decide whether a plain relative specifier must also be offered alongside
/// it (see `Project::auto_import_specifier_needs_relative_fallback`).
pub(super) fn package_import_specifiers_for_target_detailed(
    package_json_text: &str,
    package_dir: &str,
    target_file: &str,
    allow_importing_ts_extensions: bool,
    additional_targets: &[String],
) -> Vec<(String, bool)> {
    let Some(package_json) = serde_json::from_str::<serde_json::Value>(package_json_text).ok()
    else {
        return Vec::new();
    };

    let Some(imports) = package_json
        .get("imports")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    let package_type_module = package_json
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|v| v == "module");

    let package_dir = normalize_path(Path::new(package_dir));
    let target_path = strip_js_ts_extension(Path::new(target_file));
    let target_normalized = path_to_string(&target_path).replace('\\', "/");

    let mut specs = Vec::new();

    for (specifier_pattern, target_mapping) in imports {
        if !specifier_pattern.starts_with('#') {
            continue;
        }

        let ambiguous = import_entry_is_conditionally_ambiguous(target_mapping);
        // For an ambiguous (multi-branch) entry, only the first non-`types`
        // condition — object key order, which `serde_json`'s
        // `preserve_order` feature keeps as source order — is treated as
        // reachable via the bare `#pattern` specifier. The sibling branches
        // (e.g. `default`) exist to name what a *different* condition set
        // would resolve to, not additional files this same specifier
        // addresses; matching every branch would offer the same `#pattern`
        // text for several unrelated files with no way for a reader to tell
        // which one they'd actually get.
        let target_patterns = primary_import_targets(target_mapping);
        for target_pattern in target_patterns {
            let target_pattern = target_pattern.replace('\\', "/");
            if !target_pattern.starts_with("./") {
                continue;
            }

            let resolved = normalize_path(&package_dir.join(&target_pattern));
            let resolved_stripped =
                path_to_string(&strip_js_ts_extension(&resolved)).replace('\\', "/");

            let is_prefix_mapping = !specifier_pattern.contains('*')
                && !target_pattern.contains('*')
                && specifier_pattern.ends_with('/')
                && target_pattern.ends_with('/');
            let direct_capture =
                wildcard_capture_case_insensitive(&resolved_stripped, &target_normalized).or_else(
                    || {
                        if is_prefix_mapping {
                            prefix_capture_case_insensitive(&resolved_stripped, &target_normalized)
                        } else {
                            None
                        }
                    },
                );
            let additional_capture = additional_targets.iter().find_map(|candidate| {
                wildcard_capture_case_insensitive(&resolved_stripped, candidate).or_else(|| {
                    if is_prefix_mapping {
                        prefix_capture_case_insensitive(&resolved_stripped, candidate)
                    } else {
                        None
                    }
                })
            });
            let matched_via_additional_target =
                direct_capture.is_none() && additional_capture.is_some();
            let capture = direct_capture.or(additional_capture);
            let Some(capture) = capture else {
                continue;
            };

            let mut specifier =
                if let Some(specifier) = apply_wildcard_capture(specifier_pattern, &capture) {
                    specifier
                } else if is_prefix_mapping {
                    format!("{specifier_pattern}{capture}")
                } else {
                    continue;
                };

            if (specifier_pattern.contains('*') || is_prefix_mapping)
                && !specifier_pattern.ends_with(".js")
                && !specifier_pattern.ends_with(".ts")
                && !has_source_extension(&target_pattern)
                && !has_source_extension(&specifier)
            {
                let prefer_ts_extension = allow_importing_ts_extensions
                    && !matched_via_additional_target
                    && (specifier_pattern.contains('/')
                        || (package_type_module && resolved_stripped.contains("/src/")));
                if prefer_ts_extension {
                    if let Some(ext) = ts_source_extension(target_file) {
                        specifier.push_str(ext);
                    } else {
                        specifier.push_str(".js");
                    }
                } else {
                    specifier.push_str(".js");
                }
            }

            specs.push((specifier, ambiguous));
        }
    }

    dedup_specifier_pairs_in_place(&mut specs);
    specs.sort_by(|(a, _), (b, _)| compare_module_specifier_candidates(a, b));
    specs
}

fn dedup_specifier_pairs_in_place(v: &mut Vec<(String, bool)>) {
    let mut seen = FxHashSet::default();
    v.retain(|(specifier, _)| seen.insert(specifier.clone()));
}

pub(super) fn collect_import_targets(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(text) => vec![text.to_string()],
        serde_json::Value::Array(items) => items.iter().flat_map(collect_import_targets).collect(),
        serde_json::Value::Object(map) => map.values().flat_map(collect_import_targets).collect(),
        _ => Vec::new(),
    }
}

/// The target(s) a bare `#pattern` specifier is treated as reaching for
/// auto-import purposes: for a plain string/array target, every listed
/// alternative (unchanged from [`collect_import_targets`]); for a
/// conditions object, only the first non-`types` key's branch — object
/// key order, preserved by `serde_json`'s `preserve_order` feature, mirrors
/// source order in the `imports` map. See
/// [`import_entry_is_conditionally_ambiguous`] for why the remaining
/// branches are not additional reachable files.
fn primary_import_targets(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Object(map) => map
            .iter()
            .find(|(key, _)| key.as_str() != "types")
            .map_or_else(Vec::new, |(_, branch)| collect_import_targets(branch)),
        other => collect_import_targets(other),
    }
}

/// Whether an `imports` entry's target resolves to more than one distinct
/// physical file depending on which condition is active (e.g. `browser` vs
/// `default`). The `types` branch is excluded: it names a declaration-file
/// counterpart of the *same* target, not a different reachable file, so its
/// presence alongside a single `default`/unconditional branch must not read
/// as ambiguous (`{"types": "./types/*", "default": "./dist/*"}` names one
/// file, not two).
fn import_entry_is_conditionally_ambiguous(value: &serde_json::Value) -> bool {
    let serde_json::Value::Object(map) = value else {
        return false;
    };
    let mut leaves: Vec<String> = map
        .iter()
        .filter(|(key, _)| key.as_str() != "types")
        .flat_map(|(_, branch)| collect_import_targets(branch))
        .collect();
    leaves.sort();
    leaves.dedup();
    leaves.len() > 1
}

/// Whether `target_file` is reachable only through a *non-canonical* branch
/// of a conditionally ambiguous `imports` entry — e.g. `node.ts` under
/// `"#is-browser": {"browser": "./dist/env/browser.js", "default":
/// "./dist/env/node.js"}`, where `browser.ts` (the first non-`types`
/// condition) is [`primary_import_targets`]'s pick. Such a file already has
/// a same-name, same-shape sibling declaration surfaced under the
/// `#pattern` specifier (plus its own relative fallback, since that
/// specifier doesn't reliably address it — see
/// `Project::auto_import_specifier_needs_relative_fallback`); listing it
/// again under its own plain relative specifier would offer three ways to
/// import what reads, at the call site, as a single conceptual export.
pub(super) fn is_ambiguous_import_shadow_target(
    package_json_text: &str,
    package_dir: &str,
    target_file: &str,
    additional_targets: &[String],
) -> bool {
    let Some(package_json) = serde_json::from_str::<serde_json::Value>(package_json_text).ok()
    else {
        return false;
    };
    let Some(imports) = package_json
        .get("imports")
        .and_then(serde_json::Value::as_object)
    else {
        return false;
    };

    let package_dir = normalize_path(Path::new(package_dir));
    let target_path = strip_js_ts_extension(Path::new(target_file));
    let target_normalized = path_to_string(&target_path).replace('\\', "/");

    for target_mapping in imports.values() {
        if !import_entry_is_conditionally_ambiguous(target_mapping) {
            continue;
        }
        let serde_json::Value::Object(conditions) = target_mapping else {
            continue;
        };
        let primary = primary_import_targets(target_mapping);

        for (key, branch) in conditions {
            if key == "types" {
                continue;
            }
            let branch_leaves = collect_import_targets(branch);
            if branch_leaves.iter().all(|leaf| primary.contains(leaf)) {
                continue; // This is the primary branch itself.
            }

            for target_pattern in branch_leaves {
                let target_pattern = target_pattern.replace('\\', "/");
                if !target_pattern.starts_with("./") {
                    continue;
                }
                let resolved = normalize_path(&package_dir.join(&target_pattern));
                let resolved_stripped =
                    path_to_string(&strip_js_ts_extension(&resolved)).replace('\\', "/");

                let matches_direct =
                    wildcard_capture_case_insensitive(&resolved_stripped, &target_normalized)
                        .is_some();
                let matches_additional = additional_targets.iter().any(|candidate| {
                    wildcard_capture_case_insensitive(&resolved_stripped, candidate).is_some()
                });
                if matches_direct || matches_additional {
                    return true;
                }
            }
        }
    }

    false
}

pub(super) fn collect_exports_targets(
    value: &serde_json::Value,
    mode: ExportsResolutionMode,
) -> (Vec<String>, Vec<String>) {
    let mut types = Vec::new();
    let mut defaults = Vec::new();
    collect_exports_targets_inner(value, false, mode, &mut types, &mut defaults);
    (types, defaults)
}

pub(super) fn collect_exports_targets_inner(
    value: &serde_json::Value,
    is_types_branch: bool,
    mode: ExportsResolutionMode,
    types: &mut Vec<String>,
    defaults: &mut Vec<String>,
) {
    match value {
        serde_json::Value::String(text) => {
            if is_types_branch {
                types.push(text.to_string());
            } else {
                defaults.push(text.to_string());
            }
        }
        serde_json::Value::Array(items) => {
            // Per Node's resolution algorithm, only the FIRST array element
            // that yields a resolvable target should be used. Recurse into
            // items one at a time and stop once either target list grows,
            // matching tsserver's exports-map behavior for alternates.
            let initial_types = types.len();
            let initial_defaults = defaults.len();
            for item in items {
                collect_exports_targets_inner(item, is_types_branch, mode, types, defaults);
                if types.len() > initial_types || defaults.len() > initial_defaults {
                    break;
                }
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                let key_is_types = key == "types";
                let include_default_branch = match key.as_str() {
                    "types" => false,
                    "import" => mode != ExportsResolutionMode::Require,
                    "require" => mode != ExportsResolutionMode::Import,
                    // Preserve fallback behavior for `default`, subpath maps, and
                    // unknown conditions by treating them as available.
                    _ => true,
                };
                if !key_is_types && !include_default_branch {
                    continue;
                }
                collect_exports_targets_inner(
                    item,
                    is_types_branch || key_is_types,
                    mode,
                    types,
                    defaults,
                );
            }
        }
        _ => {}
    }
}

pub(super) fn apply_wildcard_capture(specifier_pattern: &str, capture: &str) -> Option<String> {
    if let Some((prefix, suffix)) = specifier_pattern.split_once('*') {
        let mut spec = String::with_capacity(prefix.len() + capture.len() + suffix.len());
        spec.push_str(prefix);
        spec.push_str(capture);
        spec.push_str(suffix);
        return Some(spec);
    }

    if capture.is_empty() {
        return Some(specifier_pattern.to_string());
    }

    None
}

pub(super) fn wildcard_capture_case_insensitive(pattern: &str, target: &str) -> Option<String> {
    fn capture(pattern: &str, target: &str) -> Option<String> {
        let pattern_lower = pattern.to_ascii_lowercase();
        let target_lower = target.to_ascii_lowercase();
        if let Some((prefix, suffix)) = pattern_lower.split_once('*') {
            if !target_lower.starts_with(prefix) || !target_lower.ends_with(suffix) {
                return None;
            }
            let start = prefix.len();
            let end = target_lower.len().saturating_sub(suffix.len());
            return Some(target[start..end].to_string());
        }
        (pattern_lower == target_lower).then_some(String::new())
    }

    let pattern = pattern.replace('\\', "/");
    let target = target.replace('\\', "/");

    capture(&pattern, &target)
        .or_else(|| pattern.strip_prefix('/').and_then(|p| capture(p, &target)))
        .or_else(|| target.strip_prefix('/').and_then(|t| capture(&pattern, t)))
        .or_else(|| {
            pattern
                .strip_prefix('/')
                .zip(target.strip_prefix('/'))
                .and_then(|(p, t)| capture(p, t))
        })
}

pub(super) fn prefix_capture_case_insensitive(
    prefix_pattern: &str,
    target: &str,
) -> Option<String> {
    let pattern = prefix_pattern.replace('\\', "/");
    let target = target.replace('\\', "/");
    let pattern = pattern.trim_end_matches('/');

    if pattern.is_empty() {
        return None;
    }

    fn capture(pattern: &str, target: &str) -> Option<String> {
        let pattern_lower = pattern.to_ascii_lowercase();
        let target_lower = target.to_ascii_lowercase();
        if target_lower == pattern_lower {
            return Some(String::new());
        }
        if !target_lower.starts_with(&pattern_lower) {
            return None;
        }
        let rest = target.get(pattern.len()..)?;
        let rest = rest.strip_prefix('/')?;
        Some(rest.to_string())
    }

    capture(pattern, &target)
        .or_else(|| pattern.strip_prefix('/').and_then(|p| capture(p, &target)))
        .or_else(|| target.strip_prefix('/').and_then(|t| capture(pattern, t)))
        .or_else(|| {
            pattern
                .strip_prefix('/')
                .zip(target.strip_prefix('/'))
                .and_then(|(p, t)| capture(p, t))
        })
}

pub(super) fn base_dir_for_compiler_options(
    config_dir: &Path,
    compiler_options: &serde_json::Map<String, serde_json::Value>,
) -> PathBuf {
    let base_url = compiler_options
        .get("baseUrl")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(".");
    normalize_path(&config_dir.join(base_url))
}

pub(super) fn resolve_path_mapping_target(
    mapped_target: &str,
    base_dir: &Path,
    config_dir: &Path,
) -> String {
    let mapped_target = mapped_target.replace('\\', "/");
    let resolved = if let Some(rest) = mapped_target.strip_prefix("${configDir}/") {
        path_to_string(&normalize_path(&config_dir.join(rest))).replace('\\', "/")
    } else {
        path_to_string(&normalize_path(&base_dir.join(&mapped_target))).replace('\\', "/")
    };
    path_to_string(&strip_js_ts_extension(Path::new(&resolved))).replace('\\', "/")
}

pub(super) fn strip_js_ts_extension(path: &Path) -> PathBuf {
    strip_path_file_extension(path, strip_known_extension)
}

/// Returns the runtime (emit) extension for a source file path, preserving
/// the ESM/CJS flavor. `.mts`/`.d.mts`/`.mjs` → `.mjs`, `.cts`/`.d.cts`/`.cjs`
/// → `.cjs`, everything else → `.js`.
pub(super) fn runtime_extension_for_source_path(path: &str) -> &'static str {
    let normalized = path.replace('\\', "/");
    if normalized.ends_with(".d.mts")
        || normalized.ends_with(".mts")
        || normalized.ends_with(".mjs")
    {
        return ".mjs";
    }
    if normalized.ends_with(".d.cts")
        || normalized.ends_with(".cts")
        || normalized.ends_with(".cjs")
    {
        return ".cjs";
    }
    ".js"
}

pub(super) fn has_source_extension(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.ends_with(".d.ts")
        || normalized.ends_with(".d.mts")
        || normalized.ends_with(".d.cts")
        || normalized.ends_with(".ts")
        || normalized.ends_with(".tsx")
        || normalized.ends_with(".mts")
        || normalized.ends_with(".cts")
        || normalized.ends_with(".js")
        || normalized.ends_with(".jsx")
        || normalized.ends_with(".mjs")
        || normalized.ends_with(".cjs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_collapses_dot_segments_and_clamps_at_root() {
        // Pinned before routing through path_identity::normalize_segments.
        // Every caller joins onto a package/config directory, so the inputs
        // are effectively rooted; these shapes must not change.
        assert_eq!(
            normalize_path(Path::new("/pkg/./lib/../x")),
            PathBuf::from("/pkg/x")
        );
        // Excess `..` clamps at the filesystem root (both the historical loop
        // and the canonical helper agree here).
        assert_eq!(normalize_path(Path::new("/a/../../b")), PathBuf::from("/b"));
        assert_eq!(
            normalize_path(Path::new("/cfg/dir/../paths/target")),
            PathBuf::from("/cfg/paths/target")
        );
        // Canonical semantics for a relative input (unreachable from the
        // rooted call sites): an unmatched `..` is kept, where the
        // historical loop silently dropped it (`x`).
        assert_eq!(normalize_path(Path::new("../x")), PathBuf::from("../x"));
    }

    #[test]
    fn strip_ts_path_extension_uses_shared_ts_family_rules() {
        assert_eq!(
            strip_ts_path_extension(Path::new("src/types.d.cts")),
            PathBuf::from("src/types")
        );
        assert_eq!(
            strip_ts_path_extension(Path::new("src/types.d.tsx")),
            PathBuf::from("src/types.d")
        );
        assert_eq!(
            strip_ts_path_extension(Path::new("src/runtime.mjs")),
            PathBuf::from("src/runtime.mjs")
        );
    }

    #[test]
    fn strip_js_ts_extension_uses_shared_known_extension_rules() {
        assert_eq!(
            strip_js_ts_extension(Path::new("src/runtime.mjs")),
            PathBuf::from("src/runtime")
        );
        assert_eq!(
            strip_js_ts_extension(Path::new("src/types.d.tsx")),
            PathBuf::from("src/types.d")
        );
    }
}
