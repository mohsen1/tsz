//! Embedded standard-library loading for the stdio LSP server.
//!
//! The `tsz-lsp` `Project` is library-agnostic: it accepts parsed-and-bound
//! `LibFile`s via `Project::set_lib_files` but does not own any lib assets.
//! This module is the host-side glue that resolves the embedded ECMAScript /
//! DOM lib set for the effective `target`/`lib`, parses and binds each lib once
//! (cached for the server's lifetime), and hands the shared `Arc`s to the
//! project so global values like `Date`/`Map`/`Math` resolve exactly as they do
//! in the `tsz` CLI.

use std::sync::{Arc, Mutex, OnceLock};

use rustc_hash::FxHashMap;
use tsz::config::{
    apply_explicit_lib_aliases, default_lib_name_for_target, resolve_lib_files_from_embedded,
};
use tsz::emitter::{PrinterOptions, ScriptTarget};
use tsz::lib_loader::LibFile;

/// A parsed-and-bound set of standard-library files, shared by `Arc`.
type LibFileSet = Arc<Vec<Arc<LibFile>>>;

/// Process-wide cache of `LibFileSet`s keyed by their resolved lib-file-name
/// list.
type LibFileCache = Mutex<FxHashMap<Vec<String>, LibFileSet>>;

/// The target used when no tsconfig `target` applies.
///
/// Bound to the `tsz` CLI's own default (`PrinterOptions::default().target`)
/// rather than a separate literal, so an LSP session over a file with no
/// tsconfig resolves exactly the global libs the CLI would and cannot silently
/// drift from the emit/check default.
pub(crate) fn default_lsp_target() -> ScriptTarget {
    PrinterOptions::default().target
}

/// Resolve and load the embedded lib set for the given target / explicit lib
/// list, returning the parsed-and-bound files.
///
/// Results are cached process-wide keyed by the resolved lib-file-name list, so
/// repeated installs of the same effective configuration reuse the same `Arc`s
/// (and `Project::set_lib_files` then skips the redundant per-file re-bind).
/// Parsing the full DOM lib is comparatively expensive, so this one-time-per-
/// configuration cost keeps editor latency off the hot path.
pub(crate) fn embedded_lib_files(
    target: ScriptTarget,
    explicit_lib: Option<&[String]>,
) -> LibFileSet {
    static CACHE: OnceLock<LibFileCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(FxHashMap::default()));

    let names = resolve_embedded_lib_names(target, explicit_lib);
    if names.is_empty() {
        return Arc::new(Vec::new());
    }

    if let Ok(guard) = cache.lock()
        && let Some(found) = guard.get(&names)
    {
        return Arc::clone(found);
    }

    let mut lib_files = Vec::with_capacity(names.len());
    for name in &names {
        if let Some(content) = tsz::embedded_libs::get_lib_content(name) {
            lib_files.push(Arc::new(LibFile::from_source(
                name.clone(),
                content.to_string(),
            )));
        }
    }
    let shared = Arc::new(lib_files);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(names, Arc::clone(&shared));
    }
    shared
}

/// Resolve the ordered list of embedded lib file *basenames* for a
/// configuration, following `/// <reference lib="..." />` directives.
///
/// Returns basenames such as `es2024.full.d.ts`, suitable for
/// `embedded_libs::get_lib_content`. An empty `Vec` (the embedded resolver
/// erroring on an unknown explicit lib) leaves the server lib-less rather than
/// failing — the file is still served, just without global lib symbols.
fn resolve_embedded_lib_names(
    target: ScriptTarget,
    explicit_lib: Option<&[String]>,
) -> Vec<String> {
    // `resolve_lib_files_from_embedded` does not alias internally (aliasing
    // lives in the disk-path wrappers), so apply the shared `tsc`-compatible
    // explicit-`lib` aliases (`es6`->`es2015`, `es7`->`es2016`) here. This
    // applies only to an explicit `lib`, never to `target`-derived defaults.
    let resolved = match explicit_lib {
        Some(libs) if !libs.is_empty() => {
            resolve_lib_files_from_embedded(&apply_explicit_lib_aliases(libs), true)
        }
        _ => resolve_lib_files_from_embedded(
            &[default_lib_name_for_target(target).to_string()],
            true,
        ),
    };
    match resolved {
        Ok(paths) => paths
            .iter()
            .filter_map(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
            .collect(),
        Err(_) => Vec::new(),
    }
}
