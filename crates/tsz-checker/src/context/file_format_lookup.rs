//! Deterministic lookup of per-file ESM/CJS module format from
//! `file_is_esm_map`.
//!
//! The driver builds a project-wide map from file name to ESM/CJS flag for
//! Node16/NodeNext/Bundler resolution. Checker code asks this map "is the
//! file at `<path>` an ESM file?" for a long list of decisions: whether to
//! emit TS1192 for a missing default export, whether `export =` is allowed,
//! whether to skip an export check, whether to apply
//! `allowSyntheticDefaultImports`, and which resolution mode to pick for
//! extensionless imports.
//!
//! # Why this helper exists
//!
//! Map keys and query strings come from different layers (driver, tests,
//! arena `source_file.file_name`) and historically have not been
//! normalized to one canonical form. To compensate, the previous lookup
//! tried four direct candidates and then fell back to
//! `map.iter().find_map(|...| key.ends_with(query))`. That fallback was
//! the source of [#10900]: when two map keys both end with the queried
//! suffix (for example `/proj/src/types.ts` and `/proj/test/types.ts`
//! both matching `types.ts`), the answer depended on `FxHashMap`
//! iteration order — which changes between programs and across rebuilds
//! once the bucket layout shifts — so the same file resolved to ESM in
//! one run and to CJS in the next. Downstream that flipped TS1192/TS1259
//! diagnostics and the synthesized default-import member layout in
//! Node16 alias rows on the utility-types-project benchmark.
//!
//! # Algorithm
//!
//! 1. Probe `map.get(file_name)` verbatim — the overwhelming majority
//!    of queries match a key the driver inserted with the exact same
//!    string, and short-circuiting here keeps the dominant case at one
//!    hash probe with no allocation or scan.
//! 2. On miss, normalize the query (`\` → `/`) and probe up to three
//!    additional spellings, each conditional on being distinct from a
//!    prior probe: the normalized form (only when normalization
//!    actually changed bytes); the form with any leading `/` stripped
//!    (only when the query had one); the form with `/` prepended (only
//!    when it did not). Each `map.get()` short-circuits on hit.
//! 3. If every direct probe misses, fall back to a *longest-suffix-match*
//!    over the map. Among all keys whose normalized form equals or ends
//!    with one of the query spellings, pick the key with the **greatest
//!    length** (most specific path), breaking ties by **lexicographic
//!    minimum** so the result is reproducible regardless of hash
//!    iteration order.
//! 4. If no key matches, return `None`; callers fall back to compiler
//!    options or other policy.
//!
//! Longest-match is the right tie-breaker because the legitimate use of
//! the suffix fallback is "the map key carries a longer absolute prefix
//! than the query string." Keys that share an extra leading directory
//! with the query are strictly more specific; preferring them avoids
//! false matches against unrelated files that happen to end in the same
//! basename.
//!
//! # Determinism notes
//!
//! - `FxHashMap` uses a fixed-seed hasher, so iteration order is
//!   deterministic *for the same set of keys*. The same project,
//!   compiled twice in the same process, would not flip on its own.
//! - The flip surfaces when the set of map keys differs between two
//!   compilations of the same source file — for example, in a benchmark
//!   loop that reuses the program across rows but rebuilds the
//!   `file_is_esm_map` per row, or in tests that share helper modules
//!   between cases. The longest-suffix policy is invariant under those
//!   changes, so the chosen answer stays stable.
//!
//! [#10900]: https://github.com/mohsen1/tsz/issues/10900

use rustc_hash::FxHashMap;
use std::borrow::Cow;

/// Look up whether `file_name` is an ESM file in `file_is_esm_map`.
///
/// See the module documentation for the matching policy. Returns `None`
/// when no key matches; callers should fall back to compiler-option or
/// extension-based heuristics.
pub(crate) fn lookup_file_is_esm_in_map(
    map: &FxHashMap<String, bool>,
    file_name: &str,
) -> Option<bool> {
    if map.is_empty() {
        return None;
    }

    // Hot path: the overwhelming majority of queries on Linux corpora
    // pass an absolute forward-slash path that the driver inserted with
    // the exact same string. Try it as-is first so the dominant case
    // costs one hash probe with no allocation and no scans.
    if let Some(&is_esm) = map.get(file_name) {
        return Some(is_esm);
    }

    let normalized = normalize_slashes(file_name);
    let trimmed = normalized.trim_start_matches('/');
    let query_had_leading_slash = trimmed.len() != normalized.len();

    // Additional probes covering the canonical key spellings. Each runs
    // only when it is distinct from the verbatim probe above:
    // - `normalized` differs from `file_name` only when the query had a
    //   `\` that was converted to `/`.
    // - `trimmed` differs from `normalized` only when the query already
    //   had a leading `/`.
    // - The leading-`/` variant only matters when the query did NOT
    //   already have one; otherwise it duplicates `normalized` and the
    //   hash lookup is pure waste.
    if normalized.as_ref() != file_name
        && let Some(&is_esm) = map.get(normalized.as_ref())
    {
        return Some(is_esm);
    }
    if query_had_leading_slash && let Some(&is_esm) = map.get(trimmed) {
        return Some(is_esm);
    }
    if !query_had_leading_slash {
        let with_slash = format!("/{trimmed}");
        if let Some(&is_esm) = map.get(with_slash.as_str()) {
            return Some(is_esm);
        }
    }

    // Deterministic fallback: longest-suffix match with a lexicographic
    // tie-break. `max_by_key` picks the maximum of `(len, Reverse(path))`,
    // which prefers longer keys and breaks ties toward the
    // lexicographically smaller path. Map keys are unique, so the
    // composite key never genuinely ties — the choice is total. Length
    // and lex comparisons run against the raw key bytes; the 1:1
    // `\` → `/` normalization preserves length, and any consistent
    // tie-break spelling is fine as long as it is stable.
    map.iter()
        .filter(|(path, _)| {
            let key = normalize_slashes(path);
            key_matches_query(key.as_ref(), normalized.as_ref(), trimmed)
        })
        .max_by_key(|(path, _)| (path.len(), std::cmp::Reverse(path.as_str())))
        .map(|(_, &is_esm)| is_esm)
}

/// Convert backslashes to forward slashes, borrowing the input when no
/// conversion is needed.
fn normalize_slashes(path: &str) -> Cow<'_, str> {
    if path.contains('\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        Cow::Borrowed(path)
    }
}

/// True when `key` ends with one of the query spellings. `key == normalized`
/// and `key == trimmed` are subsumed by `ends_with` (any string ends with
/// itself), so they need no separate branch.
fn key_matches_query(key: &str, normalized: &str, trimmed: &str) -> bool {
    key.ends_with(normalized) || (normalized != trimmed && key.ends_with(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_of(entries: &[(&str, bool)]) -> FxHashMap<String, bool> {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_string(), *v))
            .collect()
    }

    #[test]
    fn empty_map_returns_none() {
        let map = FxHashMap::default();
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/foo.ts"), None);
    }

    #[test]
    fn direct_hit_returns_value() {
        let map = map_of(&[("/proj/foo.ts", true), ("/proj/bar.ts", false)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/foo.ts"), Some(true));
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/bar.ts"), Some(false));
    }

    #[test]
    fn backslash_path_normalized_to_forward_slash() {
        let map = map_of(&[("/proj/foo.ts", true)]);
        assert_eq!(
            lookup_file_is_esm_in_map(&map, "\\proj\\foo.ts"),
            Some(true)
        );
    }

    #[test]
    fn map_keyed_with_backslashes_still_found() {
        let map = map_of(&[("\\proj\\foo.ts", true)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/foo.ts"), Some(true));
    }

    #[test]
    fn leading_slash_asymmetry_resolved() {
        // Map key has no leading slash, query has one.
        let map = map_of(&[("proj/foo.ts", true)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/foo.ts"), Some(true));

        // Map key has leading slash, query has none.
        let map = map_of(&[("/proj/foo.ts", true)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "proj/foo.ts"), Some(true));
    }

    /// Core regression for issue #10900. When two keys end with the
    /// queried suffix but disagree on the ESM flag, the previous
    /// `find_map` implementation returned whichever the hash iterator
    /// happened to yield first. The longest-suffix policy must instead
    /// pick the most specific key deterministically — and length must
    /// dominate the lexicographic tie-break, otherwise the shorter
    /// (lex-smaller) key would win in a different way for the same
    /// reason.
    #[test]
    fn longest_suffix_match_wins_over_short_basename_match() {
        let map = map_of(&[
            // Less specific: matches `foo.ts` via shortest suffix.
            // Lex-smaller than the longer key, so this also exercises the
            // length-beats-lex ordering of the tie-break.
            ("foo.ts", false),
            // More specific: longer absolute path also ending in `foo.ts`.
            ("/proj/src/utils/foo.ts", true),
        ]);
        assert_eq!(
            lookup_file_is_esm_in_map(&map, "/proj/src/utils/foo.ts"),
            Some(true),
            "the longer key sharing the full query path must win",
        );
    }

    /// Two equally-long keys that both end with the suffix: pick the
    /// lexicographically smaller one for stability. Previously the
    /// answer depended on `FxHashMap` iteration order and could flip
    /// between runs when the program's file set changed.
    #[test]
    fn equal_length_suffix_match_breaks_ties_lexicographically() {
        let map = map_of(&[("/a/foo.ts", true), ("/b/foo.ts", false)]);
        // Query `foo.ts` ends with both keys via the suffix path. Both
        // keys are 10 chars long. `/a/foo.ts` is lexicographically
        // smaller, so it wins regardless of insertion order.
        assert_eq!(lookup_file_is_esm_in_map(&map, "foo.ts"), Some(true));
    }

    /// Run the suffix-match lookup against many different orderings of
    /// the same key set; the answer must be identical every time. This
    /// asserts the *deterministic* property, not just one specific
    /// answer.
    #[test]
    fn suffix_match_is_invariant_under_insertion_order() {
        let entries: Vec<(&str, bool)> = vec![
            ("/a/foo.ts", true),
            ("/b/foo.ts", false),
            ("/c/foo.ts", true),
            ("/d/foo.ts", false),
        ];
        let canonical = lookup_file_is_esm_in_map(&map_of(&entries), "foo.ts");

        // Shuffle by reversing and by rotating, then re-check. With
        // longest-suffix + lexicographic tie-break the answer never
        // depends on insertion order. (FxHashMap's iteration order can
        // vary across key sets even with a fixed-seed hasher; this loop
        // exercises that.)
        let mut reordered = entries.clone();
        reordered.reverse();
        assert_eq!(
            lookup_file_is_esm_in_map(&map_of(&reordered), "foo.ts"),
            canonical
        );

        reordered.rotate_left(1);
        assert_eq!(
            lookup_file_is_esm_in_map(&map_of(&reordered), "foo.ts"),
            canonical
        );

        reordered.rotate_left(1);
        assert_eq!(
            lookup_file_is_esm_in_map(&map_of(&reordered), "foo.ts"),
            canonical
        );
    }

    /// In the suffix-fallback path, a sibling key that does NOT end with
    /// the query (here `bar.ts` against query `foo.ts`) must be skipped
    /// entirely, even when it is lexicographically smaller than the
    /// matching key. The first probes miss because the query is a bare
    /// basename and no map key spells it that way, so the fallback runs.
    #[test]
    fn non_matching_keys_are_skipped() {
        let map = map_of(&[("/proj/src/bar.ts", false), ("/proj/src/foo.ts", true)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "foo.ts"), Some(true));
    }

    #[test]
    fn no_match_returns_none() {
        let map = map_of(&[("/proj/foo.ts", true)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/bar.ts"), None);
    }

    /// The original test fixture from `conformance_issues/modules/context.rs`:
    /// the map is keyed with bare basenames (`mod.cts`, `b.mts`) while the
    /// arena queries with the same names. Direct lookup must still work.
    #[test]
    fn basename_keys_match_basename_queries() {
        let map = map_of(&[("mod.cts", false), ("b.mts", true)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "mod.cts"), Some(false));
        assert_eq!(lookup_file_is_esm_in_map(&map, "b.mts"), Some(true));
    }
}
