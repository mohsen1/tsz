//! Lib file resolution.
//!
//! Resolving `compilerOptions.lib` / `--target`-derived lib names to concrete
//! `.d.ts` file paths, lib-directory discovery, the embedded-lib fallback, and
//! `/// <reference lib="..." />` directive extraction.
//!
//! Extracted from `config/mod.rs` to separate lib-resolution from option
//! parsing/validation and keep each file under the 2000-line limit
//! (§19; config domain split tracked by #8280).

use anyhow::{Context, Result, anyhow, bail};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::env;
use std::path::{Path, PathBuf};

use crate::emitter::ScriptTarget;

/// Resolve lib files from names, optionally following `/// <reference lib="..." />` directives.
///
/// When `follow_references` is true, each lib file is scanned for reference directives
/// and those referenced libs are also loaded. When false, only the explicitly listed
/// libs are loaded without following their internal references.
///
/// TypeScript always follows `/// <reference lib="..." />` directives when loading libs.
/// For example, `lib.dom.d.ts` references `es2015` and `es2018.asynciterable`, so even
/// `--target es5` (which loads lib.d.ts -> dom) transitively loads ES2015 features.
/// Verified with `tsc 6.0.0-dev --target es5 --listFiles`.
pub fn resolve_lib_files_with_options(
    lib_list: &[String],
    follow_references: bool,
) -> Result<Vec<PathBuf>> {
    resolve_lib_files_with_options_inner(lib_list, follow_references, true)
}

/// Like `resolve_lib_files_with_options` but treats the input list as transitive:
/// unknown lib names are silently skipped instead of erroring. Use this for libs
/// pulled in from `/// <reference lib="..." />` directives in user source files,
/// where the lib catalog may have drifted across TS versions (e.g. rxjs still
/// references the long-renamed `esnext.asynciterable`).
pub fn resolve_lib_files_with_options_transitive(
    lib_list: &[String],
    follow_references: bool,
) -> Result<Vec<PathBuf>> {
    resolve_lib_files_with_options_inner(lib_list, follow_references, false)
}

fn resolve_lib_files_with_options_inner(
    lib_list: &[String],
    follow_references: bool,
    initial_is_required: bool,
) -> Result<Vec<PathBuf>> {
    if lib_list.is_empty() {
        return Ok(Vec::new());
    }

    if should_use_embedded_libs() {
        return resolve_lib_files_from_embedded_inner(
            lib_list,
            follow_references,
            initial_is_required,
        );
    }

    match default_lib_dir() {
        Ok(lib_dir) => resolve_lib_files_from_dir_inner(
            lib_list,
            follow_references,
            initial_is_required,
            &lib_dir,
        ),
        Err(_) => {
            resolve_lib_files_from_embedded_inner(lib_list, follow_references, initial_is_required)
        }
    }
}

pub fn resolve_lib_files_from_dir_with_options(
    lib_list: &[String],
    follow_references: bool,
    lib_dir: &Path,
) -> Result<Vec<PathBuf>> {
    resolve_lib_files_from_dir_inner(lib_list, follow_references, true, lib_dir)
}

fn resolve_lib_files_from_dir_inner(
    lib_list: &[String],
    follow_references: bool,
    initial_is_required: bool,
    lib_dir: &Path,
) -> Result<Vec<PathBuf>> {
    if lib_list.is_empty() {
        return Ok(Vec::new());
    }

    let lib_map = build_lib_map(lib_dir)?;
    let mut resolved = Vec::new();
    // (lib_name, is_initial) — initial entries come from user compilerOptions.lib
    // and must resolve; transitive entries come from `/// <reference lib="..." />`
    // directives inside lib files (or user sources) and are skipped silently when
    // unknown, matching `tsc` behavior. This matters in practice: rxjs and other
    // older libraries reference libs like `esnext.asynciterable` that have since
    // been renamed/folded into newer lib names.
    let mut pending: VecDeque<(String, bool)> = lib_list
        .iter()
        .map(|value| (normalize_lib_name(value), initial_is_required))
        .collect();
    let mut visited = FxHashSet::default();

    while let Some((lib_name, is_required)) = pending.pop_front() {
        if lib_name.is_empty() || !visited.insert(lib_name.clone()) {
            continue;
        }

        let path = match lib_map.get(&lib_name) {
            Some(path) => path.clone(),
            None => {
                if is_required {
                    return Err(anyhow!(
                        "unsupported compilerOptions.lib '{}' (not found in {})",
                        lib_name,
                        lib_dir.display()
                    ));
                }
                continue;
            }
        };
        resolved.push(path.clone());

        // Only follow /// <reference lib="..." /> directives if requested
        if follow_references {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read lib file {}", path.display()))?;
            for reference in extract_lib_references(&contents) {
                pending.push_back((reference, false));
            }
        }
    }

    Ok(resolved)
}

/// Resolve lib files from names, following `/// <reference lib="..." />` directives.
/// This is used when explicitly specifying libs via `--lib`.
///
/// Applies tsc-compatible aliases: `es6` → `es2015`, `es7` → `es2016`.
/// In tsc, `--lib es6` maps to `lib.es2015.d.ts` (NOT `lib.es6.d.ts`).
/// `lib.es6.d.ts` is a "full" umbrella that includes DOM/scripthost references,
/// while `lib.es2015.d.ts` only includes ES2015 language features.
/// This aliasing only applies to explicit `--lib` (not `--target`-derived defaults).
pub fn resolve_lib_files(lib_list: &[String]) -> Result<Vec<PathBuf>> {
    let aliased = apply_explicit_lib_aliases(lib_list);
    resolve_lib_files_with_options(&aliased, true)
}

pub fn resolve_lib_files_from_dir(lib_list: &[String], lib_dir: &Path) -> Result<Vec<PathBuf>> {
    let aliased = apply_explicit_lib_aliases(lib_list);
    resolve_lib_files_from_dir_with_options(&aliased, true, lib_dir)
}

/// Apply tsc-compatible aliases for user-supplied `--lib` names.
///
/// In tsc's `commandLineParser.ts`, the `libs` array maps:
/// - `es6` → `lib.es2015.d.ts`
/// - `es7` → `lib.es2016.d.ts`
///
/// This is NOT applied for `--target`-derived default libs, where `es6`
/// correctly refers to `lib.es6.d.ts` (which includes DOM).
fn apply_explicit_lib_aliases(lib_list: &[String]) -> Vec<String> {
    lib_list
        .iter()
        .map(|name| match name.to_ascii_lowercase().as_str() {
            "es6" => "es2015".to_string(),
            "es7" => "es2016".to_string(),
            _ => name.clone(),
        })
        .collect()
}

/// Resolve default lib files for a given target.
///
/// Matches tsc's behavior exactly:
/// 1. Get the root lib file for the target (e.g., "lib" for ES5, "es2015.full" for ES2015)
/// 2. Follow ALL `/// <reference lib="..." />` directives recursively
///
/// This means `--target es5` loads lib.d.ts -> dom -> es2015 (transitively),
/// which is exactly what tsc does (verified with `tsc --target es5 --listFiles`).
pub fn resolve_default_lib_files(target: ScriptTarget) -> Result<Vec<PathBuf>> {
    let root_lib = default_lib_name_for_target(target);
    if should_use_embedded_libs() {
        return resolve_lib_files_from_embedded(&[root_lib.to_string()], true);
    }

    match default_lib_dir() {
        Ok(lib_dir) => resolve_default_lib_files_from_dir(target, &lib_dir),
        Err(_) => resolve_lib_files_from_embedded(&[root_lib.to_string()], true),
    }
}

pub fn resolve_default_lib_files_from_dir(
    target: ScriptTarget,
    lib_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let root_lib = default_lib_name_for_target(target);
    // Use the raw (un-aliased) resolver — default libs from --target should
    // use lib.es6.d.ts (which includes DOM), not lib.es2015.d.ts.
    resolve_lib_files_from_dir_with_options(&[root_lib.to_string()], true, lib_dir)
}

/// Get the default lib name for a target.
///
/// This matches tsc's default behavior exactly:
/// - Each target loads the corresponding `.full` lib which includes:
///   - The ES version libs (e.g., es5, es2015.promise, etc.)
///   - DOM types (document, window, console, fetch, etc.)
///   - `ScriptHost` types
///
/// The mapping matches TypeScript's `getDefaultLibFileName()` in utilitiesPublic.ts:
/// - ES3/ES5 → lib.d.ts (npm) / es5.full.d.ts (source tree)
/// - ES2015  → lib.es6.d.ts (npm) / es2015.full.d.ts (source tree)
/// - ES2016+ → lib.es20XX.full.d.ts
/// - `ESNext`  → lib.esnext.full.d.ts
///
/// Note: The source tree uses `es5.full.d.ts` naming, while built TypeScript uses `lib.d.ts`.
/// We use the source tree naming since that's what exists in TypeScript/src/lib.
pub const fn default_lib_name_for_target(target: ScriptTarget) -> &'static str {
    match target {
        // ES3/ES5 -> lib.d.ts (npm) or es5.full.d.ts (source tree)
        ScriptTarget::ES3 | ScriptTarget::ES5 => "lib",
        // ES2015 -> lib.es6.d.ts (npm) or es2015.full.d.ts (source tree)
        ScriptTarget::ES2015 => "es6",
        // ES2016+ use .full variants (ES + DOM + ScriptHost + others)
        ScriptTarget::ES2016 => "es2016.full",
        ScriptTarget::ES2017 => "es2017.full",
        ScriptTarget::ES2018 => "es2018.full",
        ScriptTarget::ES2019 => "es2019.full",
        ScriptTarget::ES2020 => "es2020.full",
        ScriptTarget::ES2021 => "es2021.full",
        ScriptTarget::ES2022 => "es2022.full",
        ScriptTarget::ES2023 => "es2023.full",
        ScriptTarget::ES2024 => "es2024.full",
        // ES2025 and ESNext use esnext.full which includes experimental features
        ScriptTarget::ES2025 | ScriptTarget::ESNext => "esnext.full",
    }
}

/// Get the core lib name for a target (without DOM/ScriptHost).
///
/// This is useful for conformance testing where:
/// 1. Tests don't need DOM types
/// 2. Core libs are smaller and faster to load
/// 3. Tests that need DOM should specify @lib: dom explicitly
pub const fn core_lib_name_for_target(target: ScriptTarget) -> &'static str {
    match target {
        ScriptTarget::ES3 | ScriptTarget::ES5 => "es5",
        ScriptTarget::ES2015 => "es2015",
        ScriptTarget::ES2016 => "es2016",
        ScriptTarget::ES2017 => "es2017",
        ScriptTarget::ES2018 => "es2018",
        ScriptTarget::ES2019 => "es2019",
        ScriptTarget::ES2020 => "es2020",
        ScriptTarget::ES2021 => "es2021",
        ScriptTarget::ES2022 => "es2022",
        ScriptTarget::ES2023
        | ScriptTarget::ES2024
        | ScriptTarget::ES2025
        | ScriptTarget::ESNext => "esnext",
    }
}

/// Get the default lib directory.
///
/// Searches in order:
/// 1. `TSZ_LIB_DIR` environment variable
/// 2. Relative to the executable
/// 3. Relative to current working directory
/// 4. `TypeScript/src/lib` in the source tree
///
/// Cache for `default_lib_dir()` result. The lib directory is determined by
/// environment variables and filesystem probing that don't change during a
/// process lifetime.
static DEFAULT_LIB_DIR_CACHE: std::sync::OnceLock<Result<PathBuf, String>> =
    std::sync::OnceLock::new();

fn should_use_embedded_libs() -> bool {
    if env::var_os("TSZ_LIB_DIR").is_some() {
        return false;
    }

    env::var_os("TSZ_USE_EMBEDDED_LIBS").is_some_and(|value| {
        let normalized = value.to_string_lossy().trim().to_ascii_lowercase();
        !matches!(normalized.as_str(), "" | "0" | "false" | "no" | "off")
    })
}

pub fn default_lib_dir() -> Result<PathBuf> {
    let cached =
        DEFAULT_LIB_DIR_CACHE.get_or_init(|| default_lib_dir_uncached().map_err(|e| e.to_string()));
    match cached {
        Ok(path) => Ok(path.clone()),
        Err(msg) => bail!("{msg}"),
    }
}

fn default_lib_dir_uncached() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("TSZ_LIB_DIR") {
        let dir = PathBuf::from(dir);
        if !dir.is_dir() {
            bail!(
                "TSZ_LIB_DIR does not point to a directory: {}",
                dir.display()
            );
        }
        return Ok(canonicalize_or_owned(&dir));
    }

    if let Some(dir) = lib_dir_from_exe() {
        return Ok(dir);
    }

    if let Some(dir) = lib_dir_from_cwd() {
        return Ok(dir);
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    if let Some(dir) = lib_dir_from_root(manifest_dir) {
        return Ok(dir);
    }

    // If manifest dir is a crate under a workspace, also check ancestor dirs
    // (e.g., crates/tsz-core/ -> repo root where TypeScript/ lives)
    let mut ancestor = manifest_dir.parent();
    while let Some(dir) = ancestor {
        if let Some(found) = lib_dir_from_root(dir) {
            return Ok(found);
        }
        ancestor = dir.parent();
    }

    bail!("lib directory not found under {}", manifest_dir.display());
}

fn lib_dir_from_exe() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidate = exe_dir.join("lib");
    if candidate.is_dir() {
        return Some(canonicalize_or_owned(&candidate));
    }
    lib_dir_from_root(exe_dir)
}

fn lib_dir_from_cwd() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    lib_dir_from_root(&cwd)
}

fn lib_dir_from_root(root: &Path) -> Option<PathBuf> {
    let candidates = [
        // Built/compiled libs from tsc build output (highest priority)
        root.join("TypeScript").join("built").join("local"),
        root.join("TypeScript").join("lib"),
        // npm-installed TypeScript libs (self-contained, matching tsc's shipped format).
        // Prefer these over TypeScript/src/lib which has source-format files with
        // cross-module /// <reference lib> directives that pull in ES2015+ content
        // even for ES5 targets (e.g., dom.generated.d.ts references es2015.symbol.d.ts).
        root.join("node_modules").join("typescript").join("lib"),
        root.join("scripts")
            .join("node_modules")
            .join("typescript")
            .join("lib"),
        root.join("scripts")
            .join("emit")
            .join("node_modules")
            .join("typescript")
            .join("lib"),
        // Bundled lib snapshot committed with tsz for standalone and test environments.
        root.join("crates")
            .join("tsz-website")
            .join("src")
            .join("lib"),
        root.join("TypeScript").join("src").join("lib"),
        root.join("TypeScript")
            .join("node_modules")
            .join("typescript")
            .join("lib"),
        root.join("tests").join("lib"),
    ];

    for candidate in candidates {
        if candidate.is_dir() {
            return Some(canonicalize_or_owned(&candidate));
        }
    }

    None
}

/// Sentinel directory used for embedded lib paths when no physical lib directory exists.
/// The parallel pipeline checks basenames against embedded libs, so the directory
/// component is irrelevant — it just needs to be a valid path prefix.
const EMBEDDED_LIB_DIR: &str = "/embedded-lib";

/// Resolve lib files using embedded (compiled-in) lib content.
/// Fallback when no physical TypeScript lib directory is available.
pub fn resolve_lib_files_from_embedded(
    lib_list: &[String],
    follow_references: bool,
) -> Result<Vec<PathBuf>> {
    resolve_lib_files_from_embedded_inner(lib_list, follow_references, true)
}

fn resolve_lib_files_from_embedded_inner(
    lib_list: &[String],
    follow_references: bool,
    initial_is_required: bool,
) -> Result<Vec<PathBuf>> {
    if lib_list.is_empty() {
        return Ok(Vec::new());
    }

    let lib_map = build_lib_map_from_embedded();
    let embedded_dir = Path::new(EMBEDDED_LIB_DIR);
    let mut resolved = Vec::new();
    // See sibling `resolve_lib_files_from_dir_with_options` for why transitive
    // references are skipped silently when unknown.
    let mut pending: VecDeque<(String, bool)> = lib_list
        .iter()
        .map(|value| (normalize_lib_name(value), initial_is_required))
        .collect();
    let mut visited = FxHashSet::default();

    while let Some((lib_name, is_required)) = pending.pop_front() {
        if lib_name.is_empty() || !visited.insert(lib_name.clone()) {
            continue;
        }

        let filename = match lib_map.get(lib_name.as_str()) {
            Some(f) => *f,
            None => {
                if is_required {
                    return Err(anyhow!(
                        "unsupported compilerOptions.lib '{lib_name}' (not found in embedded libs)",
                    ));
                }
                continue;
            }
        };
        resolved.push(embedded_dir.join(filename));

        if follow_references && let Some(content) = crate::embedded_libs::get_lib_content(filename)
        {
            for reference in extract_lib_references(content) {
                pending.push_back((reference, false));
            }
        }
    }

    Ok(resolved)
}

/// Build a lib-name → filename map from embedded libs.
/// Mirrors `build_lib_map` but uses compiled-in filenames instead of directory listing.
fn build_lib_map_from_embedded() -> FxHashMap<&'static str, &'static str> {
    let mut map = FxHashMap::default();
    for filename in crate::embedded_libs::all_lib_filenames() {
        if !filename.ends_with(".d.ts") {
            continue;
        }
        let stem = filename.trim_end_matches(".d.ts");
        let stem = stem.strip_suffix(".generated").unwrap_or(stem);
        let key = stem.strip_prefix("lib.").unwrap_or(stem);
        map.insert(key, filename);
    }
    // Add fallback aliases for source tree naming (no lib.d.ts or lib.es6.d.ts):
    //   "lib" -> es5.full.d.ts, "es6" -> es2015.full.d.ts
    if !map.contains_key("lib")
        && let Some(&es5_full) = map.get("es5.full")
    {
        map.insert("lib", es5_full);
    }
    if !map.contains_key("es6")
        && let Some(&es2015_full) = map.get("es2015.full")
    {
        map.insert("es6", es2015_full);
    }
    // Apply tsc's backward-compatibility lib aliases (see `legacy_lib_aliases`
    // and the file-based `build_lib_map_uncached` for the full rationale).
    for (alias, target) in legacy_lib_aliases() {
        if !map.contains_key(*alias)
            && let Some(&filename) = map.get(*target)
        {
            map.insert(*alias, filename);
        }
    }
    map
}

/// Cache for `build_lib_map` results. The lib directory is typically resolved
/// once per process and the same map is reused for all lib resolution calls.
/// Without caching, `build_lib_map` was called once per lib being resolved,
/// each time re-reading the directory and calling `realpath` on every `.d.ts`
/// file (~110 files). This dominated total compilation time (>90% on macOS).
///
/// This is intentionally immutable after first initialization to avoid mutable
/// process-wide cache state in config loading paths.
type LibMapEntry = (PathBuf, FxHashMap<String, PathBuf>);
static LIB_MAP_CACHE: std::sync::OnceLock<LibMapEntry> = std::sync::OnceLock::new();

fn build_lib_map(lib_dir: &Path) -> Result<FxHashMap<String, PathBuf>> {
    // Fast path: return cached map if lib_dir matches
    if let Some((cached_dir, cached_map)) = LIB_MAP_CACHE.get()
        && cached_dir == lib_dir
    {
        return Ok(cached_map.clone());
    }

    let map = build_lib_map_uncached(lib_dir)?;

    // Cache first successful result. If another directory seeded the cache
    // earlier, we still return the freshly computed map for this call.
    let _ = LIB_MAP_CACHE.set((lib_dir.to_path_buf(), map.clone()));

    Ok(map)
}

fn build_lib_map_uncached(lib_dir: &Path) -> Result<FxHashMap<String, PathBuf>> {
    let mut map = FxHashMap::default();
    for entry in std::fs::read_dir(lib_dir)
        .with_context(|| format!("failed to read lib directory {}", lib_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".d.ts") {
            continue;
        }

        let stem = file_name.trim_end_matches(".d.ts");
        let stem = stem.strip_suffix(".generated").unwrap_or(stem);
        let key = normalize_lib_name(stem);
        map.insert(key, canonicalize_or_owned(&path));
    }

    // In TypeScript source tree (v6+), `lib.d.ts` and `lib.es6.d.ts` don't exist.
    // Add fallback aliases so that default target lib names resolve correctly:
    //   "lib" (ES5 default) -> es5.full.d.ts
    //   "es6" (ES2015 default) -> es2015.full.d.ts
    if !map.contains_key("lib")
        && let Some(path) = map.get("es5.full").cloned()
    {
        map.insert("lib".to_string(), path);
    }
    if !map.contains_key("es6")
        && let Some(path) = map.get("es2015.full").cloned()
    {
        map.insert("es6".to_string(), path);
    }

    // Apply tsc's backward-compatibility aliases for libs that were renamed
    // when their feature stabilized out of esnext. Source: TypeScript's
    // `libEntries` array in `compiler/commandLineParser.ts`. Old code that
    // still says `/// <reference lib="esnext.asynciterable" />` (e.g. rxjs)
    // must keep working.
    for (alias, target) in legacy_lib_aliases() {
        if !map.contains_key(*alias)
            && let Some(path) = map.get(*target).cloned()
        {
            map.insert((*alias).to_string(), path);
        }
    }

    Ok(map)
}

/// Backward-compat lib name aliases applied at lookup time.
/// Mirrors the tail of tsc's `libEntries` table (the "Fallback for backward
/// compatibility" block).
const fn legacy_lib_aliases() -> &'static [(&'static str, &'static str)] {
    &[
        ("es6", "es2015"),
        ("es7", "es2016"),
        ("esnext.asynciterable", "es2018.asynciterable"),
        ("esnext.symbol", "es2019.symbol"),
        ("esnext.bigint", "es2020.bigint"),
        ("esnext.weakref", "es2021.weakref"),
        ("esnext.object", "es2024.object"),
        ("esnext.regexp", "es2024.regexp"),
        ("esnext.string", "es2024.string"),
        ("esnext.float16", "es2025.float16"),
        ("esnext.iterator", "es2025.iterator"),
        ("esnext.promise", "es2025.promise"),
    ]
}

/// Extract /// <reference lib="..." /> directives from a source file.
/// Returns a list of normalized referenced lib names.
pub fn extract_lib_references(source: &str) -> Vec<String> {
    extract_lib_references_with_positions(source)
        .into_iter()
        .map(|reference| normalize_lib_name(&reference.raw))
        .collect()
}

/// A `/// <reference lib="..." />` directive captured from a source file,
/// with the byte position of the `lib` attribute value. The raw value is
/// returned exactly as it appeared between the quotes (including empty),
/// so callers can render `tsc`-compatible diagnostics like
/// `Cannot find lib definition for '<value>'.`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibReference {
    /// Raw, un-normalized lib attribute value.
    pub raw: String,
    /// Byte offset within the source where the value starts (immediately
    /// after the opening quote).
    pub start: u32,
    /// Byte length of the raw value (zero for an empty `lib=""`).
    pub length: u32,
}

/// Like [`extract_lib_references`], but returns the original (un-normalized)
/// lib values together with their byte position in the source. Used by the
/// driver to report `TS2726` for invalid user-authored source-file
/// directives while still feeding the transitive lib resolver.
pub fn extract_lib_references_with_positions(source: &str) -> Vec<LibReference> {
    let mut refs = Vec::new();
    let mut in_block_comment = false;
    let bytes = source.as_bytes();
    let mut line_start: usize = 0;
    loop {
        let line_end = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |idx| line_start + idx);
        let line_with_cr = &source[line_start..line_end];
        let line = line_with_cr.strip_suffix('\r').unwrap_or(line_with_cr);
        let trimmed = line.trim_start();
        let trim_offset = line.len() - trimmed.len();
        let trimmed_abs = line_start + trim_offset;

        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
        } else if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
        } else if trimmed.is_empty() {
            // skip blank line
        } else if trimmed.starts_with("///") {
            if let Some((value, value_offset_in_trimmed)) =
                parse_reference_lib_value_with_offset(trimmed)
            {
                refs.push(LibReference {
                    raw: value.to_string(),
                    start: (trimmed_abs + value_offset_in_trimmed) as u32,
                    length: value.len() as u32,
                });
            }
        } else if trimmed.starts_with("//") {
            // skip non-triple-slash comment
        } else {
            break;
        }

        if line_end >= bytes.len() {
            break;
        }
        line_start = line_end + 1;
    }
    refs
}

fn parse_reference_lib_value_with_offset(line: &str) -> Option<(&str, usize)> {
    let mut offset = 0;
    let bytes = line.as_bytes();
    while let Some(idx) = line[offset..].find("lib=") {
        let start = offset + idx;
        if start > 0 {
            let prev = bytes[start - 1];
            if !prev.is_ascii_whitespace() && prev != b'<' {
                offset = start + 4;
                continue;
            }
        }
        let quote = *bytes.get(start + 4)?;
        if quote != b'"' && quote != b'\'' {
            offset = start + 4;
            continue;
        }
        let value_start = start + 5;
        let rest = &line[value_start..];
        let end = rest.find(quote as char)?;
        return Some((&rest[..end], value_start));
    }
    None
}

/// Returns whether `lib_name` resolves to a known TypeScript library file.
///
/// Mirrors the resolution order used by `resolve_lib_files_with_options`:
/// the on-disk lib directory (when discoverable) takes precedence over the
/// embedded fallback. Empty inputs always return `false`. Only intended for
/// validation paths that need a yes/no answer without loading the file.
pub fn is_known_lib_name(lib_name: &str) -> bool {
    let normalized = normalize_lib_name(lib_name);
    if normalized.is_empty() {
        return false;
    }
    if let Ok(lib_dir) = default_lib_dir()
        && let Ok(map) = build_lib_map(&lib_dir)
    {
        return map.contains_key(&normalized);
    }
    build_lib_map_from_embedded().contains_key(normalized.as_str())
}

fn normalize_lib_name(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    normalized
        .strip_prefix("lib.")
        .unwrap_or(normalized.as_str())
        .to_string()
}

fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// These tests cover the module-private helpers (`apply_explicit_lib_aliases`,
// `normalize_lib_name`, `legacy_lib_aliases`) that the sibling
// `config/tests/` shards cannot reach. Behavior of the public surface
// (`extract_lib_references*`, `default/core_lib_name_for_target`,
// `is_known_lib_name`, …) is already covered there.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_lib_aliases_map_es6_es7_only() {
        // `--lib es6`/`es7` map to the language-only ES2015/ES2016 libs; every
        // other name (including casing/whitespace variants) passes through.
        let input = vec![
            "es6".to_string(),
            "ES7".to_string(),
            "  es2017  ".to_string(),
            "dom".to_string(),
        ];
        assert_eq!(
            apply_explicit_lib_aliases(&input),
            vec![
                "es2015".to_string(),
                "es2016".to_string(),
                "  es2017  ".to_string(),
                "dom".to_string(),
            ]
        );
    }

    #[test]
    fn normalize_lib_name_strips_prefix_and_case() {
        assert_eq!(normalize_lib_name("lib.ES2015.Promise"), "es2015.promise");
        assert_eq!(normalize_lib_name("  ESNext  "), "esnext");
        assert_eq!(normalize_lib_name("DOM"), "dom");
        // Only a leading `lib.` is stripped, never an embedded one.
        assert_eq!(normalize_lib_name("es2015.lib.core"), "es2015.lib.core");
    }

    #[test]
    fn legacy_aliases_point_at_existing_targets() {
        // Each renamed-out-of-esnext alias must point at a stable target name,
        // and the table must stay free of self-referential entries.
        for (alias, target) in legacy_lib_aliases() {
            assert_ne!(alias, target, "alias {alias} must not map to itself");
            assert!(!target.is_empty());
        }
    }
}
