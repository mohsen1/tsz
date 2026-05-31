use super::sources::SourceEntry;
use super::*;
use std::time::Instant;

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
    let r1 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
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
    let r2 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
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
    let r3 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
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
    let r4 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
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
    let r1 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);

    // Warm the cache with a no-op second pass.
    let sources = vec![
        make_source("/a.ts", "export const a = 1;"),
        make_source("/b.ts", "export const b = 2;"),
    ];
    let r2 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
    assert_eq!(Arc::as_ptr(&r1.program), Arc::as_ptr(&r2.program));

    // Now remove one file.
    let sources = vec![make_source("/a.ts", "export const a = 1;")];
    let r3 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
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
    let r1 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);

    // Warm the cache.
    let sources = vec![make_source("/a.ts", "export const a = 1;")];
    let r2 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
    assert_eq!(Arc::as_ptr(&r1.program), Arc::as_ptr(&r2.program));

    // Add a new file.
    let sources = vec![
        make_source("/a.ts", "export const a = 1;"),
        make_source("/c.ts", "export const c = 3;"),
    ];
    let r3 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
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
    build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);

    // Warm the merge cache.
    let sources = vec![make_source("/a.ts", "export const a = 1;")];
    let r2 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
    assert!(r2.dirty_paths.is_empty());

    cache.clear();

    // After clear, next build must re-merge.
    let sources = vec![make_source("/a.ts", "export const a = 1;")];
    let r3 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
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

/// Timing evidence: on an unchanged 50-file project the fast path (second call)
/// is dramatically faster than the full merge (first call).
///
/// Run with `cargo test --lib -- merge_cache_tests::unchanged_rebuild_fast_path_timing
/// --nocapture` to see the raw numbers.
#[test]
fn unchanged_rebuild_fast_path_timing() {
    let (mut cache, libs) = fresh_cache();

    // Build a project with 50 files × 20 exports = 1 000 symbols to make the
    // merge phase measurable while keeping the test runtime short.
    let sources: Vec<SourceEntry> = (0..50)
        .map(|i| {
            let exports: String = (0..20)
                .map(|j| format!("export const sym_{i}_{j} = {j};\n"))
                .collect();
            make_source(&format!("/{i}.ts"), &exports)
        })
        .collect();

    // First call: cold cache, must parse+bind+merge.
    let t_first = Instant::now();
    build_program_with_cache(sources.clone(), &mut cache, &libs, ScriptTarget::ES2020);
    let first_us = t_first.elapsed().as_micros();

    // Second call: identical inputs — all bind results served from cache,
    // merge phase is skipped entirely (fast path: Arc::clone).
    let t_second = Instant::now();
    let r2 = build_program_with_cache(sources, &mut cache, &libs, ScriptTarget::ES2020);
    let second_us = t_second.elapsed().as_micros();

    // The fast path should return dirty_paths = empty (nothing changed).
    assert!(
        r2.dirty_paths.is_empty(),
        "second build must report no changes"
    );

    // On any reasonable machine the fast path (Arc clone + 3 integer comparisons)
    // should be at least 10× cheaper than a full parse+bind+merge pass.
    // We use 5× to give headroom for loaded CI machines.
    eprintln!(
        "merge_cache timing: first={first_us}µs (parse+bind+merge), second={second_us}µs (fast path, merge skipped)"
    );
    assert!(
        second_us * 5 < first_us || second_us < 500,
        "fast path ({second_us}µs) was not significantly faster than full merge ({first_us}µs); \
         expected at least 5× speedup or sub-500µs absolute"
    );
}
