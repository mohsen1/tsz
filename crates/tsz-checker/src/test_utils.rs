//! Shared test utilities for checker unit tests.
//!
//! Provides common parse→bind→check pipeline helpers to eliminate
//! duplicated test setup boilerplate across checker test modules.

use crate::context::{CheckerOptions, LibContext};
use crate::diagnostics::Diagnostic;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_parser::parser::ParserState;

/// Parse, bind, and type-check a TypeScript source string, returning all diagnostics.
///
/// Uses the given `CheckerOptions` and file name. Calls `set_lib_contexts(Vec::new())`
/// so tests run without lib definitions (preventing spurious TS2318 errors).
pub fn check_source(source: &str, file_name: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    check_source_with_file_is_esm(source, file_name, options, None)
}

/// Parse, bind, and type-check a source string with no lib contexts, source
/// file test pragmas enabled, and an explicit Node module file-format
/// classification.
pub fn check_source_with_file_is_esm(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    file_is_esm: Option<bool>,
) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.enable_source_file_test_pragmas();

    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.file_is_esm = file_is_esm;
    checker.check_source_file(source_file);
    checker.ctx.diagnostics.clone()
}

/// Parse, bind, and type-check a TypeScript source string with default options.
///
/// Convenience wrapper around [`check_source`] using `"test.ts"` and default options.
pub fn check_source_diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

/// Parse, bind, and type-check a JavaScript source string.
///
/// Uses `"test.js"` filename and enables `check_js`.
pub fn check_js_source_diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    )
}

/// Types that expose a diagnostic code for code-only test assertions.
pub trait HasDiagnosticCode {
    fn diagnostic_code(&self) -> u32;
}

impl HasDiagnosticCode for Diagnostic {
    fn diagnostic_code(&self) -> u32 {
        self.code
    }
}

impl<T: HasDiagnosticCode + ?Sized> HasDiagnosticCode for &T {
    fn diagnostic_code(&self) -> u32 {
        (*self).diagnostic_code()
    }
}

impl<T> HasDiagnosticCode for (u32, T) {
    fn diagnostic_code(&self) -> u32 {
        self.0
    }
}

/// Types that expose both diagnostic code and message text.
pub trait HasDiagnosticMessage: HasDiagnosticCode {
    fn diagnostic_message(&self) -> &str;
}

impl HasDiagnosticMessage for Diagnostic {
    fn diagnostic_message(&self) -> &str {
        &self.message_text
    }
}

impl<T: HasDiagnosticMessage + ?Sized> HasDiagnosticMessage for &T {
    fn diagnostic_message(&self) -> &str {
        (*self).diagnostic_message()
    }
}

impl HasDiagnosticMessage for (u32, String) {
    fn diagnostic_message(&self) -> &str {
        &self.1
    }
}

impl HasDiagnosticMessage for (u32, &str) {
    fn diagnostic_message(&self) -> &str {
        self.1
    }
}

/// Project diagnostic-like values to their diagnostic codes.
pub fn diagnostic_codes<T: HasDiagnosticCode>(diagnostics: &[T]) -> Vec<u32> {
    diagnostics
        .iter()
        .map(HasDiagnosticCode::diagnostic_code)
        .collect()
}

/// Count diagnostics with the given diagnostic code.
pub fn diagnostic_count<T: HasDiagnosticCode>(diagnostics: &[T], code: u32) -> usize {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic_code() == code)
        .count()
}

/// Count diagnostics whose code matches the supplied predicate.
pub fn diagnostic_count_where<T: HasDiagnosticCode>(
    diagnostics: &[T],
    mut matches: impl FnMut(u32) -> bool,
) -> usize {
    diagnostics
        .iter()
        .filter(|diagnostic| matches(diagnostic.diagnostic_code()))
        .count()
}

/// Borrow diagnostics with the given diagnostic code.
pub fn diagnostics_with_code<T: HasDiagnosticCode>(diagnostics: &[T], code: u32) -> Vec<&T> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic_code() == code)
        .collect()
}

/// Borrow diagnostics whose code matches the supplied predicate.
pub fn diagnostics_where<T: HasDiagnosticCode>(
    diagnostics: &[T],
    mut matches: impl FnMut(u32) -> bool,
) -> Vec<&T> {
    diagnostics
        .iter()
        .filter(|diagnostic| matches(diagnostic.diagnostic_code()))
        .collect()
}

/// Borrow diagnostics with any of the supplied diagnostic codes.
pub fn diagnostics_with_any_code<'a, T: HasDiagnosticCode>(
    diagnostics: &'a [T],
    codes: &[u32],
) -> Vec<&'a T> {
    diagnostics
        .iter()
        .filter(|diagnostic| codes.contains(&diagnostic.diagnostic_code()))
        .collect()
}

/// Borrow diagnostics excluding the supplied diagnostic codes.
pub fn diagnostics_without_codes<'a, T: HasDiagnosticCode>(
    diagnostics: &'a [T],
    excluded_codes: &[u32],
) -> Vec<&'a T> {
    diagnostics
        .iter()
        .filter(|diagnostic| !excluded_codes.contains(&diagnostic.diagnostic_code()))
        .collect()
}

/// Return whether any diagnostic has the given code.
pub fn has_diagnostic_code<T: HasDiagnosticCode>(diagnostics: &[T], code: u32) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.diagnostic_code() == code)
}

/// Return whether any diagnostic code matches the supplied predicate.
pub fn has_diagnostic_code_where<T: HasDiagnosticCode>(
    diagnostics: &[T],
    mut matches: impl FnMut(u32) -> bool,
) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| matches(diagnostic.diagnostic_code()))
}

/// Return whether any diagnostic has one of the supplied diagnostic codes.
pub fn has_any_diagnostic_code<T: HasDiagnosticCode>(diagnostics: &[T], codes: &[u32]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| codes.contains(&diagnostic.diagnostic_code()))
}

/// Return whether any diagnostic matches an arbitrary predicate.
pub fn has_diagnostic_where<T>(diagnostics: &[T], matches: impl FnMut(&T) -> bool) -> bool {
    diagnostics.iter().any(matches)
}

/// Project diagnostics to `(code, message_text)` pairs.
pub fn diagnostic_code_messages(
    diagnostics: impl IntoIterator<Item = Diagnostic>,
) -> Vec<(u32, String)> {
    diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Borrow diagnostics as `(code, message_text)` pairs.
pub fn diagnostic_code_message_refs(diagnostics: &[Diagnostic]) -> Vec<(u32, &str)> {
    diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.as_str()))
        .collect()
}

/// Borrow diagnostics with the given code as `(code, message_text)` pairs.
pub fn diagnostic_code_message_refs_with_code(
    diagnostics: &[Diagnostic],
    code: u32,
) -> Vec<(u32, &str)> {
    diagnostics_with_code(diagnostics, code)
        .into_iter()
        .map(|d| (d.code, d.message_text.as_str()))
        .collect()
}

/// Borrow diagnostic messages for diagnostics with the given code.
pub fn diagnostic_messages_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&str> {
    diagnostics_with_code(diagnostics, code)
        .into_iter()
        .map(|d| d.message_text.as_str())
        .collect()
}

/// Return whether any diagnostic has the given code and message fragment.
pub fn has_diagnostic_code_message<T: HasDiagnosticMessage>(
    diagnostics: &[T],
    code: u32,
    message_fragment: &str,
) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.diagnostic_code() == code
            && diagnostic.diagnostic_message().contains(message_fragment)
    })
}

/// Return whether any diagnostic message contains the supplied text.
pub fn has_diagnostic_message<T: HasDiagnosticMessage>(
    diagnostics: &[T],
    message_fragment: &str,
) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.diagnostic_message().contains(message_fragment))
}

/// Borrow diagnostics with the given code and message text.
pub fn diagnostics_with_code_message<'a, T: HasDiagnosticMessage>(
    diagnostics: &'a [T],
    code: u32,
    message_fragment: &str,
) -> Vec<&'a T> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.diagnostic_code() == code
                && diagnostic.diagnostic_message().contains(message_fragment)
        })
        .collect()
}

/// Borrow diagnostics with the given code and any message text.
pub fn diagnostics_with_code_any_message<'a, T: HasDiagnosticMessage>(
    diagnostics: &'a [T],
    code: u32,
    message_fragments: &[&str],
) -> Vec<&'a T> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.diagnostic_code() == code
                && message_fragments
                    .iter()
                    .any(|fragment| diagnostic.diagnostic_message().contains(fragment))
        })
        .collect()
}

/// Parse, bind, and type-check JavaScript source, returning only diagnostic codes.
///
/// The caller supplies the test file name and any additional checker options.
/// This enables both `check_js` and `allow_js` for tests that want to model a
/// checked JavaScript file even when the surrounding options are TS-oriented.
pub fn check_js_source_codes_with_options(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..options
    };
    diagnostic_codes(&check_source(source, file_name, options))
}

/// Parse, bind, and type-check JavaScript source, returning `(code, message_text)` pairs.
pub fn check_js_source_code_messages_with_options(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..options
    };
    diagnostic_code_messages(check_source(source, file_name, options))
}

/// Parse, bind, and type-check JavaScript source, returning `(code, message_text)` pairs.
pub fn check_js_source_code_messages(source: &str) -> Vec<(u32, String)> {
    check_js_source_code_messages_with_options(source, "test.js", CheckerOptions::default())
}

/// Parse, bind, and type-check source, returning only diagnostic codes.
///
/// Convenience wrapper for tests that only inspect error codes.
pub fn check_source_codes(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source_diagnostics(source))
}

/// Parse, bind, and type-check a named TypeScript source string, returning only diagnostic codes.
pub fn check_source_codes_named(source: &str, file_name: &str) -> Vec<u32> {
    diagnostic_codes(&check_source(source, file_name, CheckerOptions::default()))
}

/// Parse, bind, and type-check source, returning `(code, message_text)` pairs.
///
/// Convenience wrapper for tests that inspect both error codes and message text.
pub fn check_source_code_messages(source: &str) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_source_diagnostics(source))
}

/// Parse, bind, and type-check source with `experimental_decorators` enabled, returning codes.
pub fn check_source_codes_experimental_decorators(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    ))
}

/// Parse, bind, and type-check source with `no_unused_parameters` enabled.
pub fn check_source_no_unused_params(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_unused_parameters: true,
            ..Default::default()
        },
    )
}

/// Parse, bind, and type-check source with `no_unused_locals` enabled.
pub fn check_source_no_unused_locals(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_unused_locals: true,
            ..Default::default()
        },
    )
}

/// Parse, bind, and type-check a TypeScript source string with the given options.
///
/// Uses `"test.ts"` as the file name. Convenience wrapper for tests that need
/// custom options but not a custom file name.
pub fn check_with_options(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    check_source(source, "test.ts", options)
}

/// `(code, message_text)` projection of [`check_with_options`].
pub fn check_with_options_code_messages(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_with_options(source, options))
}

/// Canonical "strict" `CheckerOptions` for tests that opt into the
/// `strict` + `strictNullChecks` + `noImplicitAny` combo.
///
/// Many checker tests need this exact triple. The shared factory keeps a
/// single source of truth; per-test overlays should clone this and tweak
/// the fields they actually care about.
pub fn strict_checker_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    }
}

/// Parse, bind, and type-check `source` under [`strict_checker_options`].
///
/// Returns full [`Diagnostic`]s; tests that only need codes or
/// `(code, message)` pairs should use the `_codes` / `_messages` projections.
pub fn check_source_strict(source: &str) -> Vec<Diagnostic> {
    check_with_options(source, strict_checker_options())
}

/// Code-only projection of [`check_source_strict`].
pub fn check_source_strict_codes(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source_strict(source))
}

/// `(code, message_text)` projection of [`check_source_strict`].
pub fn check_source_strict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, strict_checker_options())
}

/// Strict `(code, message_text)` diagnostics excluding TS2318 missing-default-lib noise.
pub fn check_source_strict_messages_without_missing_libs(source: &str) -> Vec<(u32, String)> {
    diagnostic_code_messages(
        check_source_strict(source)
            .into_iter()
            .filter(|d| d.code != 2318),
    )
}

/// Standard `lib.d.ts` source roots probed by checker tests, ordered by
/// preference: bundled stripped assets first (smallest, fastest to parse),
/// then the full bundled assets, then the TypeScript submodule's
/// `src/lib/` directory as a final fallback.
fn lib_test_roots() -> Vec<PathBuf> {
    let m = Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![
        m.join("../tsz-core/src/lib-assets-stripped"),
        m.join("../tsz-core/src/lib-assets"),
        m.join("../../TypeScript/src/lib"),
    ]
}

/// Lib basenames that broadly cover `Promise` / `Iterable` / `Symbol` /
/// DOM / esnext typings used by checker tests. Tests that need a smaller
/// or differently-shaped set should call [`load_lib_files`] with an
/// explicit slice.
pub const DEFAULT_LIB_NAMES: &[&str] = &[
    "es5.d.ts",
    "es2015.d.ts",
    "es2015.core.d.ts",
    "es2015.collection.d.ts",
    "es2015.iterable.d.ts",
    "es2015.generator.d.ts",
    "es2015.promise.d.ts",
    "es2015.proxy.d.ts",
    "es2015.reflect.d.ts",
    "es2015.symbol.d.ts",
    "es2015.symbol.wellknown.d.ts",
    "dom.d.ts",
    "dom.generated.d.ts",
    "dom.iterable.d.ts",
    "esnext.d.ts",
];

/// Load `LibFile`s for the given basenames by probing [`lib_test_roots`]
/// in order. Names not found in any root are silently skipped — callers
/// that strictly require a particular lib should assert presence
/// themselves. Duplicates in `names` are deduped.
pub fn load_lib_files(names: &[&str]) -> Vec<Arc<LibFile>> {
    let roots = lib_test_roots();
    let mut out = Vec::new();
    let mut seen: FxHashSet<&str> = FxHashSet::default();
    for &name in names {
        if !seen.insert(name) {
            continue;
        }
        for root in &roots {
            let p = root.join(name);
            if p.exists()
                && let Ok(content) = std::fs::read_to_string(&p)
            {
                out.push(Arc::new(LibFile::from_source(name.to_string(), content)));
                break;
            }
        }
    }
    out
}

/// Convenience: load the [`DEFAULT_LIB_NAMES`] bundle.
pub fn load_default_lib_files() -> Vec<Arc<LibFile>> {
    load_lib_files(DEFAULT_LIB_NAMES)
}

/// Roots probed by [`load_compiled_lib_files`], ordered by preference.
/// These point at directories where TypeScript's own compiled lib files
/// (with the `lib.` prefix preserved, e.g. `lib.es5.d.ts`) live.
///
/// Includes paths relative to the worktree's `CARGO_MANIFEST_DIR` AND a
/// walk-up fallback to the primary checkout. `npm install` only
/// populates `scripts/node_modules/` in the primary checkout; worktrees
/// (e.g. under `<primary>/.worktrees/<name>/`) have a fresh `scripts/`
/// without `node_modules`, so the worktree-relative roots return nothing
/// and we'd fall through to the primary checkout's roots.
fn compiled_lib_test_roots() -> Vec<PathBuf> {
    let m = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut roots = vec![
        m.join("../../TypeScript/lib"),
        m.join("../tsz-website/src/lib"),
        m.join("../../scripts/conformance/node_modules/typescript/lib"),
        m.join("../../scripts/emit/node_modules/typescript/lib"),
        m.join("../../scripts/node_modules/typescript/lib"),
    ];

    // Walk up parent directories from CARGO_MANIFEST_DIR looking for any
    // ancestor that contains `scripts/node_modules/typescript/lib/`. The
    // first hit is treated as the primary checkout. 8 levels is enough to
    // cover both `<primary>/.worktrees/<name>/crates/tsz-checker` (4
    // levels) and other reasonable layouts (`<primary>/foo/bar/...`).
    let mut ancestor: Option<&Path> = Some(m);
    let marker = Path::new("scripts/node_modules/typescript/lib");
    for _ in 0..8 {
        let Some(dir) = ancestor else { break };
        let candidate = dir.join(marker);
        if candidate.exists() {
            roots.push(candidate);
            // Also expose the conformance/emit variants that may live
            // alongside the same primary's scripts/.
            roots.push(dir.join("scripts/conformance/node_modules/typescript/lib"));
            roots.push(dir.join("scripts/emit/node_modules/typescript/lib"));
            break;
        }
        ancestor = dir.parent();
    }

    roots
}

/// Load `LibFile`s using the **compiled** TypeScript lib naming
/// (`lib.<name>.d.ts`). Pass names with the `lib.` prefix already
/// included, e.g. `&["lib.es5.d.ts", "lib.es2015.symbol.d.ts"]`.
///
/// Use this helper when a test depends on the diagnostic output anchoring
/// to the compiled lib filenames — e.g. tests that assert on
/// `Diagnostic.file == "lib.es5.d.ts"` or that exercise the
/// `source.file_name.starts_with("lib.")` gate in
/// `crates/tsz-checker/src/types/queries/lib_resolution.rs`. Most tests
/// don't need this and should use [`load_lib_files`] /
/// [`load_default_lib_files`] instead — those produce smaller `LibFile`s
/// from the bundled stripped assets.
///
/// Names not found in any root are silently skipped; duplicates are
/// deduped. The resulting `LibFile.file_name` matches the input name
/// verbatim, preserving the `lib.` prefix.
pub fn load_compiled_lib_files(names: &[&str]) -> Vec<Arc<LibFile>> {
    let roots = compiled_lib_test_roots();
    let mut out = Vec::new();
    let mut seen: FxHashSet<&str> = FxHashSet::default();
    for &name in names {
        if !seen.insert(name) {
            continue;
        }
        for root in &roots {
            let p = root.join(name);
            if p.exists()
                && let Ok(content) = std::fs::read_to_string(&p)
            {
                out.push(Arc::new(LibFile::from_source(name.to_string(), content)));
                break;
            }
        }
    }
    out
}

/// Parse, bind, and type-check `source` with the given `lib_files` wired
/// into the binder and checker.
///
/// Mirrors [`check_source`] but routes through
/// [`tsz_binder::BinderState::bind_source_file_with_libs`] and
/// `Context::set_lib_contexts` / `set_actual_lib_file_count`. Use this
/// when tests rely on built-in types (`Promise`, `Array`, `Symbol`,
/// DOM, …); for tests that don't need libs, prefer [`check_source`]
/// which is faster.
///
/// Like [`check_source`], calls `enable_source_file_test_pragmas()` so
/// `// @ts-expect-error`-style pragmas are honored.
pub fn check_source_with_libs(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), source_file);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), source_file, lib_files);
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.enable_source_file_test_pragmas();

    if lib_files.is_empty() {
        checker.ctx.set_lib_contexts(Vec::new());
    } else {
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(source_file);
    checker.ctx.diagnostics.clone()
}

/// `(code, message_text)` projection of [`check_source_with_libs`].
pub fn check_source_with_libs_code_messages(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_source_with_libs(
        source, file_name, options, lib_files,
    ))
}

/// Parse, bind, and type-check a multi-file project, returning the entry
/// file's diagnostics.
///
/// `files` is `&[(file_name, source)]`. Each file is parsed and bound
/// independently; the entry file (matched by exact `file_name`) is the
/// one that drives the checker run. Module-resolution maps are built
/// from the file-name set via [`build_module_resolution_maps`], and the
/// checker's cross-arena state (`set_all_arenas` / `set_all_binders` /
/// `set_current_file_idx` / `set_resolved_module_paths` /
/// `set_resolved_modules`) is wired up before
/// `check_source_file(entry_root)`.
///
/// Use this for cross-file regression tests that rely on
/// import-resolution or cross-file symbol delegation. For tests that
/// only need a single file, prefer [`check_source`] /
/// [`check_with_options`].
///
/// Like [`check_source`], `lib_contexts` is left empty so tests run
/// without lib definitions.
pub fn check_multi_file(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<Diagnostic> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .unwrap_or_else(|| panic!("entry_file {entry_file:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        crate::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);
    checker.ctx.diagnostics.clone()
}

/// Parse, bind, and type-check a multi-file project with lib contexts loaded.
///
/// This is the lib-aware counterpart to [`check_multi_file`]. Each project
/// file is bound through [`tsz_binder::BinderState::bind_source_file_with_libs`],
/// and the checker receives matching `lib_contexts`, so regressions involving
/// local/imported names that conflict with globals (`Boolean`, `String`, ...)
/// exercise the same lookup path as project compiles.
pub fn check_multi_file_with_libs(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<Diagnostic> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        if lib_files.is_empty() {
            binder.bind_source_file(parser.get_arena(), root);
        } else {
            binder.bind_source_file_with_libs(parser.get_arena(), root, lib_files);
        }
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .unwrap_or_else(|| panic!("entry_file {entry_file:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        crate::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    if lib_files.is_empty() {
        checker.ctx.set_lib_contexts(Vec::new());
    } else {
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);
    checker.ctx.diagnostics.clone()
}

/// Test helper: parse, bind, type-check a multi-file project AND return
/// the populated `cross_file_type_params_cache` for assertion. The cache is
/// installed before the check runs and is the same `Arc<DashMap>` returned
/// to the caller, so assertions can inspect what the checker memoized
/// during the run.
///
/// Used by tests that need to prove the cross-file type-parameter
/// memoization (`PERFORMANCE_PLAN.md` §7) actually populated.
pub fn check_multi_file_with_type_params_cache(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> (Vec<Diagnostic>, crate::context::CrossFileTypeParamsCache) {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .unwrap_or_else(|| panic!("entry_file {entry_file:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        crate::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    let cache = Arc::new(dashmap::DashMap::new());
    checker.ctx.cross_file_type_params_cache = Some(Arc::clone(&cache));

    checker.check_source_file(roots[entry_idx]);
    (checker.ctx.diagnostics.clone(), cache)
}

#[cfg(test)]
mod tests {
    //! Self-tests for the `test_utils` helpers themselves.
    //!
    //! These pin the contracts that 100s of checker tests rely on:
    //! - `check_source_diagnostics` ≡ `check_source(source, "test.ts", default)`.
    //! - `check_source_codes` is a code-only projection of `check_source_diagnostics`.
    //! - `diagnostic_code_messages` is a `(code, message)` projection of diagnostics.
    //! - `check_source_code_messages` projects to (code, message) pairs.
    //! - `check_js_source_diagnostics` uses `test.js` + `check_js: true`.
    //! - `check_js_source_code_messages_with_options` uses checked-JS options.
    //! - `check_source_codes_experimental_decorators` enables the decorator flag.
    //! - `check_source_no_unused_params` / `_no_unused_locals` enable the
    //!   matching unused-detection flag.
    //! - `check_with_options` ≡ `check_source(source, "test.ts", options)`.
    use super::*;

    #[test]
    fn check_source_diagnostics_matches_explicit_default_options() {
        // The convenience wrapper must produce the same diagnostics as the
        // 3-arg `check_source` with `"test.ts"` + default options.
        let source = "interface I {} const x = new I();";
        let lhs = check_source_diagnostics(source);
        let rhs = check_source(source, "test.ts", CheckerOptions::default());
        assert_eq!(lhs.len(), rhs.len());
        let lhs_codes: Vec<u32> = lhs.iter().map(|d| d.code).collect();
        let rhs_codes: Vec<u32> = rhs.iter().map(|d| d.code).collect();
        assert_eq!(lhs_codes, rhs_codes);
    }

    #[test]
    fn check_source_codes_is_code_projection_of_diagnostics() {
        let source = "interface I {} const x = new I();";
        let diags = check_source_diagnostics(source);
        let codes = check_source_codes(source);
        let projected: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert_eq!(codes, projected);
    }

    #[test]
    fn diagnostic_code_messages_projects_owned_diagnostics() {
        let source = "interface I {} const x = new I();";
        let diags = check_source_diagnostics(source);
        let projected: Vec<(u32, String)> = diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect();
        assert_eq!(diagnostic_code_messages(diags), projected);
    }

    #[test]
    fn check_source_code_messages_projects_pairs() {
        let source = "interface I {} const x = new I();";
        let pairs = check_source_code_messages(source);
        let diags = check_source_diagnostics(source);
        assert_eq!(pairs.len(), diags.len());
        for (i, (code, msg)) in pairs.iter().enumerate() {
            assert_eq!(*code, diags[i].code);
            assert_eq!(*msg, diags[i].message_text);
        }
    }

    #[test]
    fn check_source_diagnostics_returns_empty_for_clean_source() {
        let codes = check_source_codes("const x: number = 1;");
        assert!(
            codes.is_empty(),
            "expected no diagnostics for `const x: number = 1;`, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_diagnostics_emits_ts2693_for_interface_as_value() {
        let codes = check_source_codes("interface I {} const x = new I();");
        assert!(
            codes.contains(&2693),
            "expected TS2693 for interface used as value, got: {codes:?}"
        );
    }

    #[test]
    fn check_js_source_diagnostics_uses_check_js_flag() {
        // A JS-specific diagnostic that requires `check_js: true` is the
        // simplest contract test. `function Foo(){ this.x = 1 }; new Foo()`
        // is well-typed under check_js but produces TS7006/TS7041 etc. when
        // an undeclared identifier is used. Use a source with an obvious
        // type error and confirm we see SOME diagnostics under check_js.
        let source = "var x: number = 'hi';";
        let diags = check_js_source_diagnostics(source);
        // Should NOT emit TS2322 — type annotations are syntax errors in JS
        // and the parser path produces TS8010/TS8009 instead. We just want
        // to confirm `check_js: true` was applied (the diagnostics differ
        // from the default-TS path).
        let ts_diags = check_source_diagnostics(source);
        // The two helpers have different filename + check_js flag, so the
        // diagnostic SETS should not be identical for a TS-syntax-in-JS
        // source.
        let js_codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        let ts_codes: Vec<u32> = ts_diags.iter().map(|d| d.code).collect();
        assert_ne!(
            js_codes, ts_codes,
            "JS source with TS syntax should emit different diagnostics than TS path"
        );
    }

    #[test]
    fn check_js_source_code_messages_with_options_matches_checked_js_projection() {
        let source = "var x: number = 'hi';";
        let opts = CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        };
        let pairs = check_js_source_code_messages_with_options(source, "custom.js", opts.clone());
        let explicit = check_source(
            source,
            "custom.js",
            CheckerOptions {
                allow_js: true,
                check_js: true,
                ..opts
            },
        );
        assert_eq!(pairs, diagnostic_code_messages(explicit));
    }

    #[test]
    fn check_source_no_unused_params_emits_ts6133() {
        let source = "function f(unused: number) {}";
        let diags = check_source_no_unused_params(source);
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&6133),
            "expected TS6133 for unused parameter, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_no_unused_locals_emits_ts6133() {
        let source = "function f() { var unused: number = 1; }";
        let diags = check_source_no_unused_locals(source);
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&6133),
            "expected TS6133 for unused local, got: {codes:?}"
        );
    }

    #[test]
    fn check_with_options_matches_check_source_with_test_ts() {
        // `check_with_options(source, opts)` is exactly
        // `check_source(source, "test.ts", opts)` — pin that.
        let opts = CheckerOptions {
            no_unused_parameters: true,
            ..Default::default()
        };
        let source = "function f(unused: number) {}";
        let lhs = check_with_options(source, opts.clone());
        let rhs = check_source(source, "test.ts", opts);
        let lhs_codes: Vec<u32> = lhs.iter().map(|d| d.code).collect();
        let rhs_codes: Vec<u32> = rhs.iter().map(|d| d.code).collect();
        assert_eq!(lhs_codes, rhs_codes);
    }

    #[test]
    fn check_source_codes_experimental_decorators_clean_decorator_compiles() {
        // With `experimental_decorators` enabled, a well-typed decorator
        // application must not produce diagnostics. This pins that the flag
        // gets propagated through `CheckerOptions` to the checker.
        let source = r#"
function dec(target: any) { return target; }
@dec
class C {}
"#;
        let codes = check_source_codes_experimental_decorators(source);
        // No TS1219 ("Experimental decorator") gate.
        assert!(
            !codes.contains(&1219),
            "experimental_decorators flag should suppress TS1219, got: {codes:?}"
        );
    }

    #[test]
    fn strict_checker_options_sets_canonical_triple() {
        let opts = strict_checker_options();
        assert!(opts.strict, "strict_checker_options must set strict");
        assert!(
            opts.strict_null_checks,
            "strict_checker_options must set strict_null_checks"
        );
        assert!(
            opts.no_implicit_any,
            "strict_checker_options must set no_implicit_any"
        );
        // Other fields are explicit defaults — the factory must not silently
        // turn them on (callers rely on overlay-by-spread).
        let defaults = CheckerOptions::default();
        assert_eq!(opts.strict_function_types, defaults.strict_function_types);
        assert_eq!(
            opts.exact_optional_property_types,
            defaults.exact_optional_property_types
        );
    }

    #[test]
    fn check_source_strict_matches_explicit_strict_options() {
        let source = "let s: string = 1;";
        let lhs = check_source_strict(source);
        let rhs = check_with_options(source, strict_checker_options());
        let lhs_codes: Vec<u32> = lhs.iter().map(|d| d.code).collect();
        let rhs_codes: Vec<u32> = rhs.iter().map(|d| d.code).collect();
        assert_eq!(lhs_codes, rhs_codes);
    }

    #[test]
    fn check_with_options_code_messages_projects_custom_option_diagnostics() {
        let source = "function f() { return this; }";
        let opts = CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_this: true,
            ..CheckerOptions::default()
        };
        let pairs = check_with_options_code_messages(source, opts.clone());
        let diags = check_with_options(source, opts);
        assert_eq!(pairs.len(), diags.len());
        assert!(
            pairs.iter().any(|(code, _)| *code == 2683),
            "expected custom noImplicitThis options to report TS2683, got {pairs:?}"
        );
        for (i, pair) in pairs.iter().enumerate() {
            assert_eq!(pair.0, diags[i].code);
            assert_eq!(pair.1, diags[i].message_text);
        }
    }

    #[test]
    fn check_source_strict_codes_and_messages_project_strict_diagnostics() {
        let source = "let s: string = 1;";
        let codes = check_source_strict_codes(source);
        let pairs = check_source_strict_messages(source);
        let diags = check_source_strict(source);
        assert_eq!(codes.len(), diags.len());
        assert_eq!(pairs.len(), diags.len());
        for (i, code) in codes.iter().enumerate() {
            assert_eq!(*code, diags[i].code);
            assert_eq!(pairs[i].0, diags[i].code);
            assert_eq!(pairs[i].1, diags[i].message_text);
        }
    }

    #[test]
    fn check_source_strict_emits_ts2322_for_implicit_string_to_number() {
        // strict + strictNullChecks + noImplicitAny is enough to surface the
        // TS2322 mismatch on `let s: string = 1;`.
        let codes = check_source_strict_codes("let s: string = 1;");
        assert!(
            codes.contains(&2322),
            "expected TS2322 under strict_checker_options, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_lib_contexts_are_empty_no_ts2318() {
        // The wrapper's `set_lib_contexts(Vec::new())` step prevents
        // spurious TS2318 ("Cannot find global type") errors that would
        // otherwise fire for built-in types like Promise/Array. Pin that
        // a source that uses `Promise` does NOT emit TS2318.
        let source = "let p: Promise<number>;";
        let codes = check_source_codes(source);
        assert!(
            !codes.contains(&2318),
            "set_lib_contexts(empty) must prevent TS2318 for Promise, got: {codes:?}"
        );
    }

    #[test]
    fn load_default_lib_files_finds_es5_and_es2015_promise() {
        // The DEFAULT_LIB_NAMES bundle must resolve at least the core
        // typings every checker test relies on. If the bundled
        // `lib-assets-stripped/` ever loses one of these the checker
        // tests that use Promise/Array will silently lose lib coverage.
        let libs = load_default_lib_files();
        let names: Vec<&str> = libs.iter().map(|l| l.file_name.as_str()).collect();
        assert!(
            names.contains(&"es5.d.ts"),
            "DEFAULT_LIB_NAMES must resolve es5.d.ts in some root, got: {names:?}"
        );
        assert!(
            names.contains(&"es2015.promise.d.ts"),
            "DEFAULT_LIB_NAMES must resolve es2015.promise.d.ts, got: {names:?}"
        );
    }

    #[test]
    fn load_lib_files_dedupes_and_skips_missing() {
        // Duplicates in the input must not produce duplicate LibFiles.
        // Names that don't exist in any root must be silently dropped.
        let libs = load_lib_files(&["es5.d.ts", "es5.d.ts", "definitely_missing_lib.d.ts"]);
        let names: Vec<&str> = libs.iter().map(|l| l.file_name.as_str()).collect();
        assert_eq!(names.iter().filter(|n| **n == "es5.d.ts").count(), 1);
        assert!(!names.contains(&"definitely_missing_lib.d.ts"));
    }

    #[test]
    fn check_source_with_libs_resolves_promise_no_ts2318() {
        // With libs loaded, `Promise<number>` is a known global type, so
        // checking this source must not emit TS2318. (Without libs, the
        // empty-lib wrapper avoids TS2318 by suppressing global lookups
        // entirely; with libs, the global lookup must succeed.)
        let libs = load_default_lib_files();
        assert!(!libs.is_empty(), "expected default libs to load");
        let diags = check_source_with_libs(
            "let p: Promise<number>;",
            "test.ts",
            CheckerOptions::default(),
            &libs,
        );
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            !codes.contains(&2318),
            "Promise must resolve via loaded libs, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_with_libs_code_messages_projects_diagnostics() {
        let pairs = check_source_with_libs_code_messages(
            "const x: string = 1;",
            "test.ts",
            CheckerOptions::default(),
            &[],
        );
        assert!(
            pairs
                .iter()
                .any(|(code, message)| *code == 2322 && message.contains("number")),
            "expected TS2322 code/message projection, got: {pairs:?}"
        );
    }

    #[test]
    fn check_source_with_libs_empty_matches_check_source() {
        // Calling `check_source_with_libs` with an empty slice must
        // produce the exact same diagnostics as `check_source`. This
        // pins the no-lib code path as a strict superset of the lib
        // path and guards against drift between the two helpers.
        let source = "interface I {} const x = new I();";
        let lhs = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &[]);
        let rhs = check_source(source, "test.ts", CheckerOptions::default());
        let lhs_codes: Vec<u32> = lhs.iter().map(|d| d.code).collect();
        let rhs_codes: Vec<u32> = rhs.iter().map(|d| d.code).collect();
        assert_eq!(lhs_codes, rhs_codes);
    }

    #[test]
    fn load_compiled_lib_files_preserves_lib_prefix_naming() {
        // Tests that depend on the `source.file_name.starts_with("lib.")`
        // gate at lib_resolution.rs:983 (or assert against
        // `Diagnostic.file == "lib.es5.d.ts"`) require the LibFile name
        // to retain the `lib.` prefix — load_compiled_lib_files must
        // store names verbatim. We can't assume the compiled lib roots
        // are populated in every dev environment (npm install ts under
        // scripts/, or `git submodule update` for TypeScript/lib), so
        // only assert on the *naming* if at least one file resolved.
        let libs = load_compiled_lib_files(&["lib.es5.d.ts"]);
        if let Some(lib) = libs.first() {
            assert_eq!(
                lib.file_name, "lib.es5.d.ts",
                "load_compiled_lib_files must store names with the `lib.` prefix verbatim"
            );
        }
        // Dedup contract holds even when nothing resolves.
        let dup = load_compiled_lib_files(&[
            "lib.es5.d.ts",
            "lib.es5.d.ts",
            "lib.definitely_missing.d.ts",
        ]);
        assert!(dup.len() <= 1);
    }

    #[test]
    fn load_compiled_lib_files_resolves_when_only_primary_has_node_modules() {
        // When run from a worktree under `<primary>/.worktrees/<name>/`,
        // the worktree-relative `../../scripts/node_modules/...` paths
        // resolve into the worktree's empty scripts/ tree. This test
        // ensures the helper's walk-up fallback finds the primary
        // checkout's scripts/node_modules/typescript/lib/ when at
        // least one of the standard `npm install` directories has been
        // populated above the worktree.
        //
        // Skipped silently in environments without any compiled libs.
        let libs = load_compiled_lib_files(&["lib.es5.d.ts"]);
        // No assertion when the env is missing all three install dirs;
        // this is the same robustness pattern the test above uses.
        // When the helper does find a file, it must have the `lib.`
        // prefix and be readable.
        if let Some(lib) = libs.first() {
            assert!(
                !lib.arena.source_files.is_empty(),
                "loaded LibFile must have a parsed source file"
            );
            assert!(lib.file_name.starts_with("lib."));
        }
    }
}
