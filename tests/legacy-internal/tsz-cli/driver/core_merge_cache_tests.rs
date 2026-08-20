use super::sources::SourceEntry;
use super::*;
use tsz_common::options::module_detection::ModuleDetectionKind;

fn make_source(path: &str, text: &str) -> SourceEntry {
    SourceEntry {
        path: PathBuf::from(path),
        text: Some(text.to_string()),
        is_binary: false,
        suppress_parser_diagnostics: false,
    }
}

fn fresh_cache() -> (CompilationCache, Vec<Arc<LibFile>>) {
    (CompilationCache::default(), vec![])
}

/// First call with an empty cache always runs the merge.
/// Second call with the same unchanged inputs returns the same `Arc`
/// pointer (no re-merge). Third call after a file change re-merges and
/// produces a new pointer.
#[test]
fn merge_skipped_on_unchanged_inputs_and_reruns_on_change() {
    let (mut cache, libs) = fresh_cache();

    // --- first call: cold cache, must merge ---
    let sources = vec![
        make_source("/a.ts", "export const a = 1;"),
        make_source("/b.ts", "export const b = 2;"),
    ];
    let r1 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    let ptr1 = Arc::as_ptr(&r1.program);
    assert!(
        !r1.dirty_paths.is_empty(),
        "first build should have dirty paths"
    );

    // --- second call: identical inputs, merge must be skipped ---
    let sources = vec![
        make_source("/a.ts", "export const a = 1;"),
        make_source("/b.ts", "export const b = 2;"),
    ];
    let r2 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    let ptr2 = Arc::as_ptr(&r2.program);
    assert!(
        r2.dirty_paths.is_empty(),
        "second build with unchanged inputs should have no dirty paths"
    );
    assert_eq!(
        ptr1, ptr2,
        "unchanged inputs must return the same Arc<MergedProgram> pointer"
    );

    // --- third call: one file changed, must re-merge ---
    let sources = vec![
        make_source("/a.ts", "export const a = 99;"), // changed
        make_source("/b.ts", "export const b = 2;"),
    ];
    let r3 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    let ptr3 = Arc::as_ptr(&r3.program);
    assert!(
        !r3.dirty_paths.is_empty(),
        "modified-file build should have dirty paths"
    );
    assert_ne!(
        ptr2, ptr3,
        "changed inputs must produce a new Arc<MergedProgram>"
    );

    // --- fourth call: back to unchanged after change, cache is warm again ---
    let sources = vec![
        make_source("/a.ts", "export const a = 99;"),
        make_source("/b.ts", "export const b = 2;"),
    ];
    let r4 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert!(
        r4.dirty_paths.is_empty(),
        "fourth build (re-stable) should have no dirty paths"
    );
    assert_eq!(
        Arc::as_ptr(&r3.program),
        Arc::as_ptr(&r4.program),
        "re-stable inputs must return the same Arc as the last merge"
    );
}

/// Removing a file invalidates the cached merge (file-count guard).
#[test]
fn merge_invalidated_on_file_removal() {
    let (mut cache, libs) = fresh_cache();

    let sources = vec![
        make_source("/a.ts", "export const a = 1;"),
        make_source("/b.ts", "export const b = 2;"),
    ];
    let r1 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );

    // Warm the cache with a no-op second pass.
    let sources = vec![
        make_source("/a.ts", "export const a = 1;"),
        make_source("/b.ts", "export const b = 2;"),
    ];
    let r2 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert_eq!(Arc::as_ptr(&r1.program), Arc::as_ptr(&r2.program));

    // Now remove one file.
    let sources = vec![make_source("/a.ts", "export const a = 1;")];
    let r3 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert_ne!(
        Arc::as_ptr(&r2.program),
        Arc::as_ptr(&r3.program),
        "file removal must trigger a fresh merge"
    );
}

/// Adding a file invalidates the cached merge.
#[test]
fn merge_invalidated_on_file_addition() {
    let (mut cache, libs) = fresh_cache();

    let sources = vec![make_source("/a.ts", "export const a = 1;")];
    let r1 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );

    // Warm the cache.
    let sources = vec![make_source("/a.ts", "export const a = 1;")];
    let r2 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert_eq!(Arc::as_ptr(&r1.program), Arc::as_ptr(&r2.program));

    // Add a new file.
    let sources = vec![
        make_source("/a.ts", "export const a = 1;"),
        make_source("/c.ts", "export const c = 3;"),
    ];
    let r3 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert_ne!(
        Arc::as_ptr(&r2.program),
        Arc::as_ptr(&r3.program),
        "file addition must trigger a fresh merge"
    );
}

/// `CompilationCache::clear()` removes the cached merged program.
#[test]
fn clear_invalidates_merge_cache() {
    let (mut cache, libs) = fresh_cache();

    let sources = vec![make_source("/a.ts", "export const a = 1;")];
    build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );

    // Warm the merge cache.
    let sources = vec![make_source("/a.ts", "export const a = 1;")];
    let r2 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert!(r2.dirty_paths.is_empty());

    cache.clear();

    // After clear, next build must re-merge.
    let sources = vec![make_source("/a.ts", "export const a = 1;")];
    let r3 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert!(
        !r3.dirty_paths.is_empty(),
        "first build after clear must re-parse and re-merge"
    );
    assert_ne!(
        Arc::as_ptr(&r2.program),
        Arc::as_ptr(&r3.program),
        "clear must produce a fresh Arc<MergedProgram>"
    );
}

/// `merge_bind_results_ref` is called exactly once on the first (cold) build and
/// zero times on every subsequent unchanged build (the fast path returns an
/// `Arc` clone without entering the merge function). This is the deterministic
/// proof that the skip actually fires: any regression that re-introduced the
/// unconditional merge call would make `merge_calls` return 1 on the warm build.
#[test]
fn merge_call_count_is_zero_on_unchanged_inputs() {
    let (mut cache, libs) = fresh_cache();

    let sources = vec![
        make_source("/a.ts", "export const a = 1;"),
        make_source("/b.ts", "export const b = 2;"),
    ];

    let r1 = build_program_with_cache(
        sources.clone(),
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert_eq!(r1.merge_calls, 1, "cold build must call merge exactly once");

    let r2 = build_program_with_cache(
        sources,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert_eq!(
        r2.merge_calls, 0,
        "unchanged build must skip merge (fast path: merge_calls == 0)"
    );

    // Modifying a file forces merge to run again.
    let changed = vec![
        make_source("/a.ts", "export const a = 99;"),
        make_source("/b.ts", "export const b = 2;"),
    ];
    let r3 = build_program_with_cache(
        changed.clone(),
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert_eq!(
        r3.merge_calls, 1,
        "changed-file build must call merge exactly once"
    );

    // Back to unchanged → fast path fires again.
    let r4 = build_program_with_cache(
        changed,
        &mut cache,
        &libs,
        ScriptTarget::ES2020,
        ModuleDetectionKind::default(),
    );
    assert_eq!(
        r4.merge_calls, 0,
        "re-stable (unchanged after change) build must skip merge"
    );
}
