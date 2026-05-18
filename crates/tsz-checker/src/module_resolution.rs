//! Module resolution utilities for multi-file type checking.
//!
//! This module owns the cross-file specifier → file-index mapping consumed by
//! the checker, binder integration, and server. It supports the following
//! surface forms:
//! - ES imports: `import { x } from "./module"`
//! - Require: `const x = require("./module")`
//! - Import equals: `import x = require("./module")`
//! - Dynamic import: `const x = await import("./module")`
//! - Re-exports: `export { x } from "./module"`
//! - Triple-slash reference directives: `/// <reference path="./module.ts" />`
//!
//! # Resolution pipeline (canonical)
//!
//! ```text
//! arenas  ──▶ build_file_name_index ──▶ FileNameIndex (O(N), set_all_arenas)
//! (source_file_name, specifier, index) ──▶ resolve_specifier_via_file_index ──▶ Option<file_idx>
//! ```
//!
//! `CheckerContext::resolve_import_target_from_file` is the single public
//! entry-point for the checker.  It uses `global_file_name_index` (built by
//! `set_all_arenas` or `ProgramContext`) as the primary resolver, then falls
//! back to the driver-populated `resolved_module_paths` for path-mapped and
//! package-exports entries that the structural index cannot compute.
//!
//! # Legacy map (`build_module_resolution_maps`)
//!
//! `build_module_resolution_maps` materializes the O(N²) cross-product
//! `(src_idx, specifier) → tgt_idx` map.  It is still used by:
//! - `tsz-core` driver to build the module dependency graph for topological
//!   ordering, augmentation walking, and incremental scheduling.
//! - `ProgramContext.resolved_module_paths` as the driver-computed fallback.
//!
//! Checker resolution no longer depends on this map as the primary path.
//! New checker code should call `resolve_import_target_from_file` instead of
//! querying `resolved_module_paths` directly.

use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Component, Path};
use std::sync::Arc;

use tsz_parser::parser::node::NodeArena;

// ---------------------------------------------------------------------------
// Extension tables
// ---------------------------------------------------------------------------

/// TypeScript/JavaScript file extensions in resolution priority order.
///
/// `.d.ts` (and friends) must appear before `.ts` so that stripping does not
/// leave a `.d` artifact in the stem.
const TS_EXTENSIONS: &[&str] = &[
    ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs",
    ".cjs",
];

/// Tails that mark a file as an *arbitrary-extension declaration file*: a file
/// named `<base>.d.<ext>.ts` where `<ext>` is itself a known TS/JS/JSON
/// extension. These files are addressable through their paired implementation
/// specifier (`./file.ts` → `./file.d.ts.ts`) but NOT through the stripped form
/// (`./file.d.ts`), because that form would collide with genuine declaration
/// imports the user wrote.
const ARBITRARY_EXT_TAILS: &[&str] = &[
    ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".d.js", ".d.jsx", ".d.mjs", ".d.cjs", ".d.json",
];

/// Strip a known TS/JS extension from the end of `path`. Returns `path`
/// unchanged if no known extension is present. Always returns a borrow of the
/// input — no allocation.
fn strip_ts_extension(path: &str) -> &str {
    for ext in TS_EXTENSIONS {
        if let Some(stripped) = path.strip_suffix(ext) {
            return stripped;
        }
    }
    path
}

/// Detect `<base>.d.<ext>.ts` files. These are treated specially (see the
/// `ARBITRARY_EXT_TAILS` documentation).
fn is_arbitrary_extension_declaration_file(file_name: &str) -> bool {
    let Some(without_ts) = file_name.strip_suffix(".ts") else {
        return false;
    };
    ARBITRARY_EXT_TAILS
        .iter()
        .any(|tail| without_ts.ends_with(tail))
}

/// For `<base>.d.<ext>.ts` with non-TS/JS/JSON `<ext>` (e.g. `.html`, `.vue`),
/// return the `(base, ext)` parts of the user-written specifier `<base>.<ext>`.
/// Returns `None` for any other shape.
fn arbitrary_ext_decl_user_parts(file_name: &str) -> Option<(&str, &str)> {
    let stem = file_name.strip_suffix(".ts")?;
    let (base, ext) = stem.rsplit_once(".d.")?;
    if base.is_empty() || ext.is_empty() || ext.contains('/') || ext.contains('.') {
        return None;
    }
    if is_recognized_inner_module_ext(ext) {
        return None;
    }
    Some((base, ext))
}

/// True when `ext` (no leading dot) is a TS/JS/JSON module extension that has
/// its own canonical resolution path. Used by `arbitrary_ext_decl_user_parts`
/// and the fast resolver's arbitrary-extension probe to gate them off normal
/// TypeScript surfaces.
//
// Keep in sync with `tsz_core::module_resolver::is_arbitrary_extension_declaration`
// and `tsz_core::resolution::helpers::KNOWN_EXTENSIONS` until they are unified
// in a shared crate.
fn is_recognized_inner_module_ext(ext: &str) -> bool {
    matches!(
        ext,
        "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "json" | "d"
    )
}

// ---------------------------------------------------------------------------
// CanonicalSpecifier
// ---------------------------------------------------------------------------

/// Classification of a parsed module specifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecifierKind {
    /// Starts with `./` or is exactly `.`.
    Relative,
    /// Starts with `../` or is exactly `..`.
    Parent,
    /// Starts with `/` — an absolute project-root-style path.
    Absolute,
    /// Anything else (`lodash`, `@scope/pkg`, ...).
    Bare,
}

/// Canonical representation of an import specifier after trimming, quote
/// removal, and separator normalization.
///
/// Invariants enforced by `normalize_import_specifier`:
/// - `text` is non-empty.
/// - `text` has no surrounding whitespace.
/// - `text` has no surrounding matching `'…'` or `"…"` quotes.
/// - `text` contains forward slashes only (backslashes are converted).
/// - For non-pure-dot-chain specifiers, any single trailing `/` is stripped.
///   Pure dot chains (`.`, `./`, `..`, `../`, `../..`, `../../`, …) keep the
///   exact form the user wrote, because `.` and `./` are *both* valid
///   directory-index specifiers and the resolution map registers whichever
///   form applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalSpecifier {
    /// Normalized specifier text. See type-level invariants.
    pub text: String,
    /// What kind of specifier this is.
    pub kind: SpecifierKind,
    /// True if the specifier should be resolved against a directory index
    /// (`./foo/`, `.`, `./`, `..`, `../`, `../..`, `../../`, …).
    pub is_directory_hint: bool,
}

/// Strip at most one pair of matching surrounding quotes. Only matching pairs
/// are removed; lopsided quotes are left intact to preserve diagnostic fidelity.
fn strip_surrounding_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// True when every `/`-separated segment of `s` is either `.` or `..`.
/// Returns false for the empty string.
fn is_pure_dot_chain(s: &str) -> bool {
    !s.is_empty()
        && s.split('/')
            .all(|segment| segment == "." || segment == "..")
}

/// Normalize a raw import specifier string into a `CanonicalSpecifier`.
///
/// Returns `None` for specifiers that cannot be used: empty after trimming,
/// or quotes-only. Does NOT reject bare/absolute specifiers — they are
/// classified and returned so callers can apply their own policy.
pub fn normalize_import_specifier(specifier: &str) -> Option<CanonicalSpecifier> {
    let trimmed = specifier.trim();
    let unquoted = strip_surrounding_quotes(trimmed);
    if unquoted.is_empty() {
        return None;
    }

    // Normalize path separators. Cheap fast-path when there is no backslash.
    let slashed: String = if unquoted.contains('\\') {
        unquoted.replace('\\', "/")
    } else {
        unquoted.to_string()
    };

    let kind = if slashed == "." || slashed.starts_with("./") {
        SpecifierKind::Relative
    } else if slashed == ".." || slashed.starts_with("../") {
        SpecifierKind::Parent
    } else if slashed.starts_with('/') {
        SpecifierKind::Absolute
    } else {
        SpecifierKind::Bare
    };

    // A pure dot chain is something like "." / "./" / ".." / "../" / "../.."
    // etc. We keep these exactly as written — both `.` and `./` are legitimate
    // specifiers for the current-directory index and the map registers both.
    let core_without_trailing = slashed.trim_end_matches('/');
    let dot_chain = is_pure_dot_chain(core_without_trailing);
    let has_trailing_slash = slashed.len() > 1 && slashed.ends_with('/');

    let text = if dot_chain {
        slashed
    } else if has_trailing_slash {
        let mut s = slashed;
        s.pop();
        s
    } else {
        slashed
    };

    Some(CanonicalSpecifier {
        text,
        kind,
        is_directory_hint: has_trailing_slash || dot_chain,
    })
}

// ---------------------------------------------------------------------------
// TargetIndex
// ---------------------------------------------------------------------------

/// A single target file after index extraction.
#[derive(Debug)]
struct IndexedTarget<'a> {
    /// Index of the file in the original `file_names` slice.
    tgt_idx: usize,
    /// The absolute file path (as a `Path`, borrowing the input string).
    abs_path: &'a Path,
    /// If this is a `dir/index.<ext>` file, the directory it is the index for.
    /// `None` otherwise. Declaration files and arbitrary-extension
    /// declaration files are handled here by simply not being recorded.
    index_dir: Option<&'a Path>,
}

/// Indexed view of the project's target files. Built once per compilation
/// from the file list, then reused for every source file's specifier fan-out.
///
/// This replaces the previous implementation, which re-derived per-target
/// metadata inside the inner loop of an O(n²) source × target scan.
#[derive(Debug)]
pub struct TargetIndex<'a> {
    targets: Vec<IndexedTarget<'a>>,
}

impl<'a> TargetIndex<'a> {
    /// Number of usable (non-skipped) target entries in the index.
    pub const fn len(&self) -> usize {
        self.targets.len()
    }

    /// True when the index holds no targets.
    pub const fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
}

/// Build a `TargetIndex` from a list of project file paths. The returned index
/// borrows from `file_names` for zero-copy access to path components.
pub fn build_target_index(file_names: &[String]) -> TargetIndex<'_> {
    let mut targets = Vec::with_capacity(file_names.len());
    for (idx, name) in file_names.iter().enumerate() {
        let abs_path = Path::new(name.as_str());
        let file_name = abs_path.file_name().and_then(|f| f.to_str()).unwrap_or("");

        // Skip files like `foo.d.ts.ts`. They are addressable only through the
        // implementation specifier `./foo.ts`, which in the canonical map is
        // produced when some other regular target happens to align. Registering
        // `./foo.d.ts` here would shadow real declaration-file imports.
        if is_arbitrary_extension_declaration_file(file_name) {
            continue;
        }

        // Detect directory-index targets: `<dir>/index.<ext>` files contribute
        // an additional directory-shaped specifier (`./<dir>`).
        let stem = strip_ts_extension(name.as_str());
        let index_dir = stem.strip_suffix("/index").map(Path::new);

        targets.push(IndexedTarget {
            tgt_idx: idx,
            abs_path,
            index_dir,
        });
    }
    TargetIndex { targets }
}

// ---------------------------------------------------------------------------
// Relative specifier derivation
// ---------------------------------------------------------------------------

/// Compute a canonical `./…`-style relative specifier from `from_dir` to
/// `to_file`. Returns `None` if the two paths have no common ancestor (e.g.
/// different drive roots on Windows).
///
/// The returned string:
/// - always starts with `./` or `../`,
/// - has its known TS/JS extension stripped, or for arbitrary-extension
///   declaration files (`<base>.d.<ext>.ts`, `<ext>` outside TS/JS/JSON),
///   the user-written `<base>.<ext>` form,
/// - uses `/` separators.
fn relative_specifier_for_file(from_dir: &Path, to_file: &Path) -> Option<String> {
    relative_file_specifier(from_dir, to_file).map(|r| r.stem)
}

/// Both canonical spellings of a relative file specifier: with and without the
/// known TS/JS extension. Users legitimately write both (`./foo`,
/// `./foo.js`) and both must resolve to the same target.
struct RelativeFileSpecifier {
    /// Always present. `./foo` — canonical form with extension stripped.
    stem: String,
    /// Present only when the file has a recognized extension: `./foo.js`.
    with_extension: Option<String>,
    /// Present only for `<base>.d.<ext>.ts` with `<ext>` outside TS/JS/JSON:
    /// the user-written `<base>.<ext>` form (e.g. `./component.html`).
    user_alt: Option<String>,
}

/// Compute the relative forms in a single walk of the directory path chain.
fn relative_file_specifier(from_dir: &Path, to_file: &Path) -> Option<RelativeFileSpecifier> {
    let (prefix, rel_str) = walk_relative(from_dir, to_file)?;
    let stripped = strip_ts_extension(&rel_str);
    let stem = format!("{prefix}{stripped}");
    let with_extension = if stripped.len() == rel_str.len() {
        None
    } else {
        Some(format!("{prefix}{rel_str}"))
    };

    // Arbitrary-extension declaration files (`<base>.d.<ext>.ts` with `<ext>`
    // outside TS/JS/JSON) are addressable through the user-written
    // `<base>.<ext>` form (e.g. `./component.html` for
    // `./component.d.html.ts`). The legacy `<base>.d.<ext>` stem stays
    // registered for behavior compatibility with existing fixtures; the
    // user-form is added so tsc-style imports also resolve.
    let user_alt =
        arbitrary_ext_decl_user_parts(&rel_str).map(|(base, ext)| format!("{prefix}{base}.{ext}"));

    Some(RelativeFileSpecifier {
        stem,
        with_extension,
        user_alt,
    })
}

/// Walk from `from_dir` up toward the filesystem root until `to_file` lies
/// inside the current ancestor, returning the `../…/` prefix that reaches
/// that ancestor and the residual relative path. Used by both file-target and
/// directory-target specifier derivation.
fn walk_relative(from_dir: &Path, to_path: &Path) -> Option<(String, String)> {
    if let Ok(rel) = to_path.strip_prefix(from_dir) {
        return Some(("./".to_string(), rel.to_string_lossy().into_owned()));
    }
    let mut up = 0usize;
    let mut ancestor = from_dir;
    loop {
        let parent = ancestor.parent()?;
        up += 1;
        if let Ok(rel) = to_path.strip_prefix(parent) {
            let mut prefix = String::with_capacity(3 * up);
            for _ in 0..up {
                prefix.push_str("../");
            }
            return Some((prefix, rel.to_string_lossy().into_owned()));
        }
        ancestor = parent;
    }
}

/// A directory specifier pair. `primary` is always the canonical form; `alt`
/// is only populated for pure-dot-chain specifiers (`.`, `..`, `../..`, …)
/// where the trailing-slash form is also a legitimate spelling.
struct DirectorySpecifier {
    primary: String,
    alt: Option<String>,
}

/// Compute canonical directory specifier(s) from `from_dir` to `to_dir`.
///
/// For index files (`to_dir/index.<ext>`), callers use this to derive the
/// directory-shaped specifier the user would write in their import.
fn directory_specifier(from_dir: &Path, to_dir: &Path) -> Option<DirectorySpecifier> {
    let (prefix, rel_str) = walk_relative(from_dir, to_dir)?;
    if rel_str.is_empty() {
        // The target directory IS (an ancestor of) `from_dir`. Either
        // same-directory (`./`) or pure-dot-chain ancestor (`../`, `../../`).
        // Both the dot-only form (`.`, `..`, `../..`) and the trailing-slash
        // form (`./`, `../`, `../../`) are valid spellings of the same
        // directory-index specifier; register both.
        let primary = if prefix == "./" {
            ".".to_string()
        } else {
            // prefix is "../", "../../", ... — drop the trailing slash.
            let mut s = prefix.clone();
            s.pop();
            s
        };
        return Some(DirectorySpecifier {
            primary,
            alt: Some(prefix),
        });
    }
    Some(DirectorySpecifier {
        primary: format!("{prefix}{rel_str}"),
        alt: None,
    })
}

// ---------------------------------------------------------------------------
// Resolve a single specifier against the index
// ---------------------------------------------------------------------------

/// Resolve `specifier` as imported from `source_file` against the precomputed
/// `TargetIndex`.
///
/// Used for single-specifier lookups against a `TargetIndex` (returned by
/// `build_target_index`).  For the canonical checker resolution path use
/// `resolve_specifier_via_file_index` together with the `FileNameIndex`
/// returned by `build_file_name_index`; that path avoids the O(N²)
/// cross-product materialization.
pub fn resolve_from_source(
    source_file: &str,
    specifier: &CanonicalSpecifier,
    index: &TargetIndex<'_>,
) -> Option<usize> {
    // Only project-local paths are resolvable here. Bare/absolute specifiers
    // are classified but not matched against the in-memory target set.
    if !matches!(
        specifier.kind,
        SpecifierKind::Relative | SpecifierKind::Parent
    ) {
        return None;
    }

    let src_dir = Path::new(source_file).parent()?;

    for target in &index.targets {
        if let Some(rel) = relative_file_specifier(src_dir, target.abs_path)
            && (rel.stem == specifier.text
                || rel.with_extension.as_deref() == Some(specifier.text.as_str())
                || rel.user_alt.as_deref() == Some(specifier.text.as_str()))
        {
            return Some(target.tgt_idx);
        }
        if let Some(idx_dir) = target.index_dir
            && let Some(dir) = directory_specifier(src_dir, idx_dir)
            && (dir.primary == specifier.text
                || dir.alt.as_deref() == Some(specifier.text.as_str()))
        {
            return Some(target.tgt_idx);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// build_module_resolution_maps
// ---------------------------------------------------------------------------

/// Build the flat `(source_file_idx, specifier) → target_file_idx` map and
/// the set of all recognized specifier strings.
///
/// Primary consumers are the **driver** (module dependency graph for topological
/// ordering, augmentation resolution, and incremental scheduling) and the
/// `resolved_modules` set used by checker diagnostics.
///
/// **Checker module resolution** should call
/// `CheckerContext::resolve_import_target_from_file` instead of querying this
/// map directly.  The checker uses `build_file_name_index` +
/// `resolve_specifier_via_file_index` as its primary path, falling back to the
/// driver-populated `resolved_module_paths` only for specifiers the structural
/// index cannot resolve (path mappings, package exports).
///
/// See `register_canonical_forms` for the exact set of strings registered per
/// target.
pub fn build_module_resolution_maps(
    file_names: &[String],
) -> (FxHashMap<(usize, String), usize>, FxHashSet<String>) {
    let mut map: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut modules: FxHashSet<String> = FxHashSet::default();

    let index = build_target_index(file_names);

    for (src_idx, src_name) in file_names.iter().enumerate() {
        let Some(src_dir) = Path::new(src_name.as_str()).parent() else {
            continue;
        };
        for target in &index.targets {
            register_canonical_forms(src_idx, src_dir, target, &mut map, &mut modules);
        }
    }

    (map, modules)
}

/// Register exactly the canonical specifier forms that a user might legitimately
/// write to name `target` from a source file rooted at `src_dir`.
///
/// The registered forms are:
/// 1. The extension-stripped relative file specifier
///    (`./foo`, `./lib/utils`, `../lib/utils`).
/// 2. The extension-bearing relative file specifier (`./foo.js`,
///    `./types.d.ts`). Users legitimately write the extension — especially in
///    `require(...)`, triple-slash references, and JS/CJS sources — and both
///    forms must point at the same target.
/// 3. The same-directory bare alias (`foo`) — *only* for same-directory
///    targets. This is a narrow backward-compat hook: tsc proper treats bare
///    specifiers as package imports, but historically this resolver has
///    allowed naked same-dir file names and downstream checker tests depend on
///    it. Nested bare aliases (`lib/utils`) are intentionally NOT registered;
///    they semantically collide with real package sub-paths and no test
///    requires them.
/// 4. For `dir/index.<ext>` targets, the directory-shaped specifier(s): a
///    single form for ordinary directories (`./lib`), plus the trailing-slash
///    alternate for pure dot-chain directories (`.` + `./`, `..` + `../`,
///    `../..` + `../../`, …).
/// 5. The same-directory bare alias for a directory-shaped specifier
///    (`./lib` ↔ `lib`), following the same rule as point 3.
fn register_canonical_forms(
    src_idx: usize,
    src_dir: &Path,
    target: &IndexedTarget<'_>,
    map: &mut FxHashMap<(usize, String), usize>,
    modules: &mut FxHashSet<String>,
) {
    if let Some(rel) = relative_file_specifier(src_dir, target.abs_path) {
        insert(map, modules, src_idx, rel.stem.clone(), target.tgt_idx);
        if let Some(bare) = same_directory_bare_alias(&rel.stem) {
            insert(map, modules, src_idx, bare, target.tgt_idx);
        }
        if let Some(with_ext) = rel.with_extension {
            insert(map, modules, src_idx, with_ext, target.tgt_idx);
        }
        if let Some(user_alt) = rel.user_alt {
            insert(map, modules, src_idx, user_alt, target.tgt_idx);
        }
    }

    if let Some(idx_dir) = target.index_dir
        && let Some(dir_spec) = directory_specifier(src_dir, idx_dir)
    {
        insert(
            map,
            modules,
            src_idx,
            dir_spec.primary.clone(),
            target.tgt_idx,
        );
        if let Some(alt) = dir_spec.alt {
            insert(map, modules, src_idx, alt, target.tgt_idx);
        }
        if let Some(bare) = same_directory_bare_alias(&dir_spec.primary) {
            insert(map, modules, src_idx, bare, target.tgt_idx);
        }
    }

    if let Some(idx_dir) = target.index_dir
        && let Some(package_specifier) = node_modules_package_specifier(src_dir, idx_dir)
    {
        insert(map, modules, src_idx, package_specifier, target.tgt_idx);
    }
}

/// Return a bare alias for a specifier of the form `./<name>` where `<name>`
/// does not contain `/`. Returns `None` for nested (`./a/b`), parent (`../a`),
/// dot-chain (`.`, `./`), or already-bare inputs.
fn same_directory_bare_alias(spec: &str) -> Option<String> {
    let rest = spec.strip_prefix("./")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    Some(rest.to_string())
}

fn node_modules_package_specifier(src_dir: &Path, package_dir: &Path) -> Option<String> {
    let mut current = Some(src_dir);
    while let Some(dir) = current {
        if dir.file_name().is_some_and(|name| name == "node_modules")
            && let Some(package_specifier) = package_specifier_under_node_modules(dir, package_dir)
        {
            return Some(package_specifier);
        }

        let child_node_modules = dir.join("node_modules");
        if let Some(package_specifier) =
            package_specifier_under_node_modules(&child_node_modules, package_dir)
        {
            return Some(package_specifier);
        }

        current = dir.parent();
    }

    None
}

fn package_specifier_under_node_modules(node_modules: &Path, package_dir: &Path) -> Option<String> {
    let rel = package_dir.strip_prefix(node_modules).ok()?;
    let mut components = rel.components();
    let first = match components.next()? {
        Component::Normal(part) => part.to_str()?,
        _ => return None,
    };

    if first.starts_with('@') {
        let second = match components.next()? {
            Component::Normal(part) => part.to_str()?,
            _ => return None,
        };
        if components.next().is_none() {
            return Some(format!("{first}/{second}"));
        }
        return None;
    }

    if components.next().is_none() {
        return Some(first.to_string());
    }

    None
}

fn insert(
    map: &mut FxHashMap<(usize, String), usize>,
    modules: &mut FxHashSet<String>,
    src_idx: usize,
    spec: String,
    tgt_idx: usize,
) {
    map.insert((src_idx, spec.clone()), tgt_idx);
    modules.insert(spec);
}

// ---------------------------------------------------------------------------
// Public specifier-lookup shim
// ---------------------------------------------------------------------------

/// Build lookup keys for a module specifier.
///
/// This is a thin compatibility shim. New code should prefer
/// `normalize_import_specifier` plus a single lookup against the canonical
/// map. The returned vector contains at most three entries:
///
/// 1. The canonical form (trimmed, unquoted, forward-slash, trailing-slash
///    stripped for non-dot-chain specifiers).
/// 2. The raw input, only when it differs from the canonical form. This
///    covers lookups against maps keyed on the un-normalized user string
///    (e.g. `binder.module_exports`, where keys can be raw file paths).
/// 3. The extension-stripped stem, only when the canonical has a recognized
///    TS/JS extension whose removal yields a different string. TS genuinely
///    allows `./foo.js` to resolve to `./foo.d.ts`, and some call sites
///    populate their map with stem-only keys; this single fallback covers
///    them without reintroducing the old alias explosion.
///
/// No quoted / backslash / chain / dot-chain-variant / index / bare fan-out
/// is produced.
pub fn module_specifier_candidates(specifier: &str) -> Vec<String> {
    // Robustness audit (PR #N, item 14 in
    // `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`): emit a structured
    // trace at every invocation so the rate at which callers depend on this
    // compatibility shim — and the specifiers that drive it — are visible.
    // The audit's full solution collapses the legacy map and canonical-spec
    // resolver into one entrypoint with one normalized key model; this
    // visibility hook prepares the migration by exposing call sites.
    tracing::trace!(
        site = "module_resolution::module_specifier_candidates",
        specifier = specifier,
        "module-specifier compatibility-shim lookup"
    );
    let Some(canonical) = normalize_import_specifier(specifier) else {
        // Quotes-only or empty input: the raw string is the only key the
        // caller could realistically match on.
        return vec![specifier.to_string()];
    };

    let mut out = Vec::with_capacity(3);
    out.push(canonical.text.clone());
    if canonical.text != specifier {
        out.push(specifier.to_string());
    }
    let stem = strip_ts_extension(&canonical.text);
    if stem.len() != canonical.text.len() && !out.iter().any(|s| s == stem) {
        out.push(stem.to_string());
    }
    out
}

/// Build lookup keys for a module specifier when querying *resolution-error*
/// maps keyed by the exact user-written specifier.
///
/// Unlike `module_specifier_candidates`, this helper does NOT fan out to the
/// extension-stripped stem. Resolution errors are stored against the exact
/// specifier produced by the user (e.g. `"./index.js"` is one key, `"./index"`
/// is another). Two different specifiers may resolve very differently — for
/// example, `import "./index.js"` succeeds via the synthetic `.js → .ts`
/// substitution while `import "./index"` fails with TS2835. Conflating them
/// would make the checker emit the TS2835 from the extensionless variant on
/// the line that wrote `./index.js`.
///
/// The returned candidates therefore cover only spellings of the *same*
/// specifier: the canonical normalized form (whitespace/quote/separator
/// cleanup) plus the raw input when it differs.
pub fn module_specifier_error_candidates(specifier: &str) -> Vec<String> {
    let Some(canonical) = normalize_import_specifier(specifier) else {
        return vec![specifier.to_string()];
    };

    let mut out = Vec::with_capacity(2);
    out.push(canonical.text.clone());
    if canonical.text != specifier {
        out.push(specifier.to_string());
    }
    out
}

// ---------------------------------------------------------------------------
// Fast, on-demand specifier resolution via a filename reverse index
// ---------------------------------------------------------------------------
//
// `build_module_resolution_maps` materializes an (src_idx, specifier) →
// tgt_idx map over the full source × target cross-product. That is O(N²) in
// space and time and re-walks directory components on every pair. For large
// projects (thousands of files) it is both slow and a memory explosion, and
// historically it was re-run inside the checker's import-resolution fallback
// on every miss, which dominated the CPU profile.
//
// The functions below replace that fallback with a pre-built, O(N),
// project-wide reverse index from normalized absolute file name to file
// index. At specifier-lookup time we compute the small candidate set the
// specifier could address (the direct spelling, each TS/JS extension, and
// directory-index variants) and probe the reverse index O(1) per candidate.
//
// The reverse index is shared via Arc across all per-file checker contexts,
// so the whole project pays the cost once.

/// Project-wide reverse index from normalized file name to file index.
pub type FileNameIndex = FxHashMap<String, usize>;

/// Build a reverse index `normalized_file_name -> file_idx` from a slice of
/// arenas. The keys use forward slashes only, matching the forms the
/// specifier resolver produces.
pub fn build_file_name_index(arenas: &[Arc<NodeArena>]) -> FileNameIndex {
    let mut idx: FileNameIndex = FxHashMap::default();
    idx.reserve(arenas.len());
    for (file_idx, arena) in arenas.iter().enumerate() {
        let Some(sf) = arena.source_files.first() else {
            continue;
        };
        let key = if sf.file_name.contains('\\') {
            sf.file_name.replace('\\', "/")
        } else {
            sf.file_name.clone()
        };
        idx.insert(key, file_idx);
    }
    idx
}

/// Split a forward-slash path string into segments and lexically resolve
/// `.` and `..`. Preserves a leading `/` and intentionally ignores Windows
/// drive prefixes: callers are expected to have normalized backslashes and
/// the index uses the raw arena form.
fn lexical_normalize_slash(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::with_capacity(path.matches('/').count() + 1);
    for segment in path.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    let mut out = String::with_capacity(path.len());
    if absolute {
        out.push('/');
    }
    for (i, seg) in stack.iter().enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(seg);
    }
    out
}

/// Probe the filename index with every spelling a TypeScript/JavaScript
/// specifier could plausibly address. Mirrors the matching rules encoded
/// by `register_canonical_forms`:
///
/// 1. Direct hit (`./foo.ts` when the project has `./foo.ts`).
/// 2. Extension fan-out (`./foo` → `./foo.ts`, `./foo.d.ts`, ...).
/// 3. Directory-index (`./lib` or `.` → `./lib/index.ts` / `./index.ts`).
///
/// Path components are compared as strings after lexical normalization
/// (`./` and `..` resolved purely textually). Returns the first target
/// index that matches, or `None` when no project file answers the
/// specifier.
pub fn resolve_specifier_via_file_index(
    source_file_name: &str,
    specifier: &str,
    filename_idx: &FileNameIndex,
) -> Option<usize> {
    // Normalize the source file name (forward slashes only) and grab its
    // parent directory as a string. We avoid Path::strip_prefix and
    // Path::components, which showed up as >40% of total CPU in the
    // O(N²) fallback profile.
    let src_norm = if source_file_name.contains('\\') {
        source_file_name.replace('\\', "/")
    } else {
        source_file_name.to_string()
    };
    // A bare file name with no directory component (e.g. `other.js` in a
    // test harness) has no src_dir. Treat it as the "current directory" so
    // relative specifiers like `./types` still resolve against siblings.
    let src_dir = match src_norm.rfind('/') {
        Some(slash) => &src_norm[..slash],
        None => "",
    };

    let spec_norm = if specifier.contains('\\') {
        specifier.replace('\\', "/")
    } else {
        specifier.to_string()
    };

    // Preserve the legacy map boundary: bare aliases are only supported for a
    // single same-directory segment. Nested bare specifiers are package
    // subpaths (for example `react/jsx-runtime`) and must not be reinterpreted
    // as project-relative paths after the primary resolver misses.
    if !spec_norm.starts_with("./")
        && !spec_norm.starts_with("../")
        && !spec_norm.starts_with('/')
        && spec_norm != "."
        && spec_norm != ".."
        && spec_norm.contains('/')
    {
        return None;
    }

    // Join `src_dir + '/' + specifier`, letting the lexical normalizer
    // resolve the resulting `./`, `../`, and doubled slashes. Pure
    // dot-chain specifiers (`.`, `./`, `..`, `../..`) fall through here
    // and resolve to src_dir (or an ancestor) + `/index.<ext>` below.
    let joined = if src_dir.is_empty() {
        spec_norm
    } else {
        let mut s = String::with_capacity(src_dir.len() + 1 + spec_norm.len());
        s.push_str(src_dir);
        s.push('/');
        s.push_str(&spec_norm);
        s
    };
    let base = lexical_normalize_slash(&joined);

    // Direct hit: the specifier already spells out the full path (e.g.
    // `./foo.ts` when `foo.ts` is a project file).
    if let Some(&idx) = filename_idx.get(&base) {
        return Some(idx);
    }

    // Strip a recognized TS/JS extension to get the stem, so both the
    // extensioned (`./foo.ts`) and stem (`./foo`) spellings exercise the
    // same ext fan-out. If no known extension is present, `stem` equals
    // `base`.
    let stem = strip_ts_extension(&base);
    let mut buf = String::with_capacity(stem.len() + 8);

    for ext in TS_EXTENSIONS {
        buf.clear();
        buf.push_str(stem);
        buf.push_str(ext);
        if let Some(&idx) = filename_idx.get(&buf) {
            return Some(idx);
        }
    }

    // Arbitrary-extension declaration file probe (`./component.html` →
    // `/proj/component.d.html.ts`). Additive — only fires when the standard
    // TS fan-out above missed and the specifier carries a non-TS/JS/JSON
    // trailing extension.
    if let Some((stem_base, ext)) = base.rsplit_once('.')
        && !ext.is_empty()
        && !ext.contains('/')
        && !is_recognized_inner_module_ext(ext)
    {
        buf.clear();
        buf.push_str(stem_base);
        buf.push_str(".d.");
        buf.push_str(ext);
        buf.push_str(".ts");
        if let Some(&idx) = filename_idx.get(&buf) {
            return Some(idx);
        }
    }

    // Directory-index fallback (`./lib` → `./lib/index.ts`). Don't append
    // `/index` when the base is already empty (root directory). We always
    // probe the stem, not `base`, to also cover `./lib.ts` → `./lib/index.ts`
    // (unusual, but cheap and symmetric with the extension fan-out).
    if !stem.is_empty() {
        for ext in TS_EXTENSIONS {
            buf.clear();
            buf.push_str(stem);
            buf.push_str("/index");
            buf.push_str(ext);
            if let Some(&idx) = filename_idx.get(&buf) {
                return Some(idx);
            }
        }
    }

    None
}

#[cfg(test)]
#[path = "../tests/module_resolution.rs"]
mod tests;
