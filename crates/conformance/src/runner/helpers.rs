use super::*;

#[derive(Clone, Debug)]
pub(super) struct TimedTest {
    pub(super) file: String,
    pub(super) elapsed_ms: u128,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShardWeights {
    path_weights: HashMap<String, f64>,
    hash_bucket_shard_count: usize,
    hash_bucket_weights: Vec<f64>,
}

/// Format a path relative to a base directory for display
pub(super) fn relative_display(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .map_or_else(|_| path.display().to_string(), |p| p.display().to_string())
}

/// Decide whether fingerprint-level comparison should run between the two
/// diagnostic sets.
///
/// Returns `false` whenever either side is empty. Server mode (the legacy
/// protocol in `server_pool.rs`) returns only error codes, so
/// `tsz_fingerprints` is empty there — gating on TSC alone would flag every
/// TSC fingerprint as "missing" on every test, even when the codes match.
/// The same guard covers cache entries that happen to carry codes but no
/// fingerprints: fall back to code-only comparison rather than producing
/// bogus misses.
pub(super) fn use_fingerprint_compare(
    tsc_fingerprints: &std::collections::HashSet<DiagnosticFingerprint>,
    tsz_fingerprints: &std::collections::HashSet<DiagnosticFingerprint>,
) -> bool {
    !tsc_fingerprints.is_empty() && !tsz_fingerprints.is_empty()
}

pub(super) fn is_project_config_diagnostic_code(code: u32) -> bool {
    matches!(code, 18003 | 5023 | 5057 | 5058 | 5081 | 5101 | 5102 | 5107)
}

pub(super) fn sanitize_artifact_name(path: &str) -> String {
    path.chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect()
}

/// Filter diagnostics from `.lib/` test library files out of tsz results.
///
/// Our conformance wrapper resolves `/// <reference path="/.lib/react16.d.ts" />`
/// by copying lib files into the temp dir. This lets tsz type-check them and emit
/// errors (e.g. TS2430) that tsc never sees — tsc emits TS6053 "file not found"
/// instead. Filtering these avoids false positive mismatches.
pub(super) fn is_lib_diagnostic(fp: &DiagnosticFingerprint) -> bool {
    fp.file.starts_with(".lib/")
        || fp.file.starts_with("/.lib/")
        || fp.message_key.contains("/.lib/")
        || fp.message_key.contains(".lib/")
}

pub(super) fn is_typescript_builtin_lib_path(path: &str) -> bool {
    let normalized = path.trim().replace('\\', "/").to_ascii_lowercase();
    if !normalized.ends_with(".d.ts") {
        return false;
    }
    let installed_platform_lib = (normalized.starts_with("node_modules/@typescript/typescript-")
        || normalized.contains("/node_modules/@typescript/typescript-"))
        && normalized.contains("/lib/lib.");
    let source_build_platform_lib = (normalized.starts_with("built/npm/typescript-")
        || normalized.contains("/built/npm/typescript-"))
        && normalized.contains("/lib/lib.");
    normalized.starts_with("node_modules/typescript/lib/lib.")
        || normalized.contains("/node_modules/typescript/lib/lib.")
        || normalized.starts_with("typescript/lib/lib.")
        || normalized.contains("/typescript/lib/lib.")
        || installed_platform_lib
        || source_build_platform_lib
        || normalized.starts_with("lib.")
}

pub(super) fn filter_lib_diagnostics_tsz(
    mut result: tsz_wrapper::CompilationResult,
) -> tsz_wrapper::CompilationResult {
    let had_lib = result.diagnostic_fingerprints.iter().any(is_lib_diagnostic);
    if !had_lib {
        return result;
    }
    // Collect codes that ONLY appear in .lib/ fingerprints
    let lib_only_codes: std::collections::HashSet<u32> = {
        let lib_codes: std::collections::HashSet<u32> = result
            .diagnostic_fingerprints
            .iter()
            .filter(|fp| is_lib_diagnostic(fp))
            .map(|fp| fp.code)
            .collect();
        let non_lib_codes: std::collections::HashSet<u32> = result
            .diagnostic_fingerprints
            .iter()
            .filter(|fp| !is_lib_diagnostic(fp))
            .map(|fp| fp.code)
            .collect();
        lib_codes.difference(&non_lib_codes).cloned().collect()
    };
    result
        .diagnostic_fingerprints
        .retain(|fp| !is_lib_diagnostic(fp));
    result.error_codes.retain(|c| !lib_only_codes.contains(c));
    result
}

/// Filter bundled TypeScript lib diagnostics that are unique to tsz.
///
/// `tsz` can report diagnostics from `node_modules/typescript/lib/lib.*.d.ts`
/// or `TypeScript/lib/lib.*.d.ts` while checking a conformance program. Those
/// are false positives only when TSC did not report the same bundled-lib code
/// for the test. Some passing tests legitimately compare bundled-lib
/// diagnostics, so this must not drop every such fingerprint unconditionally.
pub(super) fn filter_extra_typescript_builtin_lib_diagnostics_tsz(
    mut result: tsz_wrapper::CompilationResult,
    tsc_fps: &[DiagnosticFingerprint],
) -> tsz_wrapper::CompilationResult {
    let expected_builtin_lib_codes: std::collections::HashSet<u32> = tsc_fps
        .iter()
        .filter(|fp| is_typescript_builtin_lib_path(&fp.file))
        .map(|fp| fp.code)
        .collect();
    let removable_codes: std::collections::HashSet<u32> = result
        .diagnostic_fingerprints
        .iter()
        .filter(|fp| {
            is_typescript_builtin_lib_path(&fp.file)
                && !expected_builtin_lib_codes.contains(&fp.code)
        })
        .map(|fp| fp.code)
        .collect();
    if removable_codes.is_empty() {
        return result;
    }
    result.diagnostic_fingerprints.retain(|fp| {
        !(is_typescript_builtin_lib_path(&fp.file) && removable_codes.contains(&fp.code))
    });
    let remaining_codes: std::collections::HashSet<u32> = result
        .diagnostic_fingerprints
        .iter()
        .map(|fp| fp.code)
        .collect();
    result
        .error_codes
        .retain(|code| !removable_codes.contains(code) || remaining_codes.contains(code));
    result
}

/// Filter `.lib/` artifacts from tsc cache results.
///
/// tsc emits TS6053 for unresolved `/.lib/` references. Since our wrapper
/// resolves them, these TS6053 entries are artifacts that should not count
/// as "missing" diagnostics.
pub(super) fn filter_lib_diagnostics_tsc(
    tsc_result: &crate::tsc_results::TscResult,
) -> (Vec<u32>, Vec<DiagnosticFingerprint>) {
    let mut codes = tsc_result.error_codes.clone();
    let mut fps = tsc_result.diagnostic_fingerprints.clone();

    // Structural invariant: every fingerprint's code must also appear in
    // `error_codes`. If it doesn't, the fingerprint is usually a parser
    // artifact — typically a `--traceResolution` trace line shaped like
    // `error TSxxxx:` that our no-position regex matched but the code-list
    // regex did not. Drop such orphan fingerprints.
    //
    // Exception: a small whitelist of program-level diagnostics that tsc
    // emits at the synthetic position (`<unknown>:0:0`) WITHOUT reflecting
    // the code in the per-test `error_codes` list. These are legitimate
    // comparisons against tsz's emissions:
    //
    //   - TS2318 "Cannot find global type" — @noLib tests (see PR #578, #612).
    //   - TS2468 "Cannot find global value 'Promise'" — ES5 lib tests that
    //     use async/dynamic-import (tsc reports it as a top-level file-less
    //     error but the test case's `error_codes` only includes the
    //     file-anchored TS2705/TS2712/etc.).
    const PROGRAM_LEVEL_WHITELIST: &[u32] = &[2318, 2468];
    let code_set: std::collections::HashSet<u32> = codes.iter().copied().collect();
    fps.retain(|fp| {
        code_set.contains(&fp.code)
            || (PROGRAM_LEVEL_WHITELIST.contains(&fp.code)
                && fp.file.is_empty()
                && fp.line == 0
                && fp.column == 0)
    });

    let had_lib = fps.iter().any(is_lib_diagnostic);
    if had_lib {
        fps.retain(|fp| !is_lib_diagnostic(fp));
        // Remove TS6053 from error codes if no non-.lib/ TS6053 remains
        if !fps.iter().any(|fp| fp.code == 6053) {
            codes.retain(|c| *c != 6053);
        }
    }

    // Normalize machine-specific absolute paths in "File 'X' not found." messages.
    // The tsc cache was generated on macOS where temp dirs sit under
    // /var/folders/XX/YY/T/... — paths that escape the temp root resolve to
    // machine-specific absolute paths like "/var/folders/6z/src/harness/...".
    // Normalize those to the portable form so comparison against tsz output works.
    for fp in &mut fps {
        let normalized = tsz_wrapper::normalize_file_not_found_message_key(&fp.message_key);
        if normalized != fp.message_key {
            fp.message_key = normalized;
        }
    }

    (codes, fps)
}

/// When TSC reports only TS5024 (invalid compiler option shape), suppress
/// downstream semantic diagnostics from tsz.
///
/// In the conformance harness, the cached baseline intentionally expects only
/// TS5024 for a few option-conversion mismatch cases (for example,
/// `"\"true,false\""`` in a boolean-like option). tsz currently continues and
/// emits semantic diagnostics, which should be ignored for this category.
pub(super) fn suppress_tsz_semantic_diagnostics_after_tsc_option_error(
    tsc_codes: &[u32],
    result: &mut tsz_wrapper::CompilationResult,
) {
    if tsc_codes.len() != 1 || tsc_codes.first().copied() != Some(5024) {
        return;
    }

    result.error_codes.retain(|code| *code == 5024);
    result
        .diagnostic_fingerprints
        .retain(|fingerprint| fingerprint.code == 5024);
}

/// Compare filtered tsz and tsc diagnostics and produce a `TestResult`.
///
/// Inputs are expected to have all path-specific filtering (lib, config-level,
/// `@noLib`, option-error suppression) already applied. This helper performs
/// only the final set diff, fingerprint sort, and pass/fail classification so
/// the variant, UTF-16, and binary branches of `run_single_test` share one
/// implementation.
///
/// `options` is threaded into `TestResult::Fail` unchanged — callers that
/// intentionally drop the options map (e.g. the UTF-16 path) can pass
/// `HashMap::new()`.
pub(super) fn compare_diagnostics(
    compile_result: &tsz_wrapper::CompilationResult,
    tsc_error_codes: &[u32],
    tsc_fps: &[DiagnosticFingerprint],
    options: HashMap<String, String>,
) -> TestResult {
    let tsc_codes: std::collections::HashSet<u32> = tsc_error_codes.iter().copied().collect();
    let tsz_codes: std::collections::HashSet<u32> =
        compile_result.error_codes.iter().copied().collect();

    let missing: Vec<u32> = tsc_codes.difference(&tsz_codes).copied().collect();
    let extra: Vec<u32> = tsz_codes.difference(&tsc_codes).copied().collect();

    let tsc_fingerprints: std::collections::HashSet<DiagnosticFingerprint> =
        tsc_fps.iter().cloned().collect();
    let tsz_fingerprints: std::collections::HashSet<DiagnosticFingerprint> = compile_result
        .diagnostic_fingerprints
        .iter()
        .cloned()
        .collect();
    let use_fingerprint_compare = use_fingerprint_compare(&tsc_fingerprints, &tsz_fingerprints);

    let mut missing_fingerprints: Vec<DiagnosticFingerprint> = if use_fingerprint_compare {
        tsc_fingerprints
            .difference(&tsz_fingerprints)
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let mut extra_fingerprints: Vec<DiagnosticFingerprint> = if use_fingerprint_compare {
        tsz_fingerprints
            .difference(&tsc_fingerprints)
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let fp_sort_key = |f: &DiagnosticFingerprint| {
        (
            f.code,
            f.file.clone(),
            f.line,
            f.column,
            f.message_key.clone(),
        )
    };
    let mut expected_fingerprints = tsc_fps.to_vec();
    let mut actual_fingerprints = compile_result.diagnostic_fingerprints.clone();
    expected_fingerprints.sort_by_key(fp_sort_key);
    actual_fingerprints.sort_by_key(fp_sort_key);
    missing_fingerprints.sort_by_key(fp_sort_key);
    extra_fingerprints.sort_by_key(fp_sort_key);

    let fingerprints_match = !use_fingerprint_compare
        || (missing_fingerprints.is_empty() && extra_fingerprints.is_empty());
    if missing.is_empty() && extra.is_empty() && fingerprints_match {
        TestResult::Pass
    } else {
        let mut expected = tsc_error_codes.to_vec();
        let mut actual = compile_result.error_codes.clone();
        expected.sort_unstable();
        actual.sort_unstable();
        TestResult::Fail(Box::new(TestResultFail {
            expected,
            actual,
            missing,
            extra,
            missing_fingerprints,
            extra_fingerprints,
            expected_fingerprints,
            actual_fingerprints,
            options,
            known_failure: None,
        }))
    }
}

pub(super) fn is_appledouble_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("._"))
}

pub(super) fn stable_shard_for_path(path: &Path, test_dir: &Path, shard_count: usize) -> usize {
    let key = path
        .strip_prefix(test_dir)
        .unwrap_or(path)
        .to_string_lossy();
    let mut hash = 1_469_598_103_934_665_603_u64;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    (hash as usize) % shard_count
}

pub(super) fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(super) fn path_weight_keys(path: &Path, test_dir: &Path) -> Vec<String> {
    let full = normalized_path(path);
    let rel = path
        .strip_prefix(test_dir)
        .map(normalized_path)
        .unwrap_or_else(|_| full.clone());
    vec![rel.clone(), format!("TypeScript/tests/cases/{rel}"), full]
}

pub(super) fn valid_weight(value: f64) -> Option<f64> {
    if value.is_finite() && value > 0.0 {
        Some(value)
    } else {
        None
    }
}

pub(super) fn load_json_weights(path: &Path) -> Option<ShardWeights> {
    let data = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = match serde_json::from_str(&data) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                "failed to parse conformance shard weights {}: {err}",
                path.display()
            );
            return None;
        }
    };

    let mut weights = ShardWeights::default();

    if let Some(paths) = value
        .get("path_weights")
        .and_then(serde_json::Value::as_object)
    {
        for (path, weight) in paths {
            if let Some(weight) = weight.as_f64().and_then(valid_weight) {
                weights.path_weights.insert(path.replace('\\', "/"), weight);
            }
        }
    }

    if let Some(results) = value.get("results").and_then(serde_json::Value::as_array) {
        for result in results {
            let Some(file) = result.get("file").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Some(weight) = result
                .get("elapsed_ms")
                .or_else(|| result.get("elapsed"))
                .and_then(serde_json::Value::as_f64)
                .and_then(valid_weight)
            else {
                continue;
            };
            weights.path_weights.insert(file.replace('\\', "/"), weight);
        }
    }

    if let Some(bucket_weights) = value.get("hash_bucket_weights") {
        weights.hash_bucket_shard_count = bucket_weights
            .get("shard_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        if let Some(items) = bucket_weights
            .get("weights")
            .and_then(serde_json::Value::as_array)
        {
            weights.hash_bucket_weights = items
                .iter()
                .filter_map(|weight| weight.as_f64().and_then(valid_weight))
                .collect();
        }
    }

    Some(weights)
}

pub(super) fn historical_path_weight(
    weights: &ShardWeights,
    path: &Path,
    test_dir: &Path,
) -> Option<f64> {
    for key in path_weight_keys(path, test_dir) {
        if let Some(weight) = weights.path_weights.get(&key) {
            return Some(*weight);
        }
    }

    if weights.hash_bucket_shard_count > 0 {
        let bucket = stable_shard_for_path(path, test_dir, weights.hash_bucket_shard_count);
        if let Some(weight) = weights.hash_bucket_weights.get(bucket) {
            return Some(*weight);
        }
    }

    None
}

pub(super) fn estimated_test_weight(
    weights: Option<&ShardWeights>,
    path: &Path,
    test_dir: &Path,
) -> f64 {
    if let Some(weight) = weights.and_then(|value| historical_path_weight(value, path, test_dir)) {
        return weight;
    }

    let size_weight = std::fs::metadata(path)
        .map(|metadata| (metadata.len() as f64 / 4096.0).max(1.0))
        .unwrap_or(1.0);
    size_weight.min(100.0)
}

pub(super) fn weighted_shard_files(
    files: Vec<PathBuf>,
    test_dir: &Path,
    shard_index: usize,
    shard_count: usize,
    weights: Option<&ShardWeights>,
) -> Vec<PathBuf> {
    weighted_shards(files, test_dir, shard_count, weights)
        .into_iter()
        .nth(shard_index)
        // Keep the weighted assignment order. The runner feeds this list into a
        // bounded concurrent stream, so starting heavier tests first avoids
        // leaving a slow test to extend the tail after lighter work has drained.
        .map(|(_, selected)| selected)
        .unwrap_or_default()
}

pub(super) fn weighted_shards(
    files: Vec<PathBuf>,
    test_dir: &Path,
    shard_count: usize,
    weights: Option<&ShardWeights>,
) -> Vec<(f64, Vec<PathBuf>)> {
    let mut weighted: Vec<(PathBuf, String, f64)> = files
        .into_iter()
        .map(|path| {
            let key = path
                .strip_prefix(test_dir)
                .map(normalized_path)
                .unwrap_or_else(|_| normalized_path(&path));
            let weight = estimated_test_weight(weights, &path, test_dir);
            (path, key, weight)
        })
        .collect();
    weighted.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });

    let mut shards: Vec<(f64, Vec<PathBuf>)> =
        (0..shard_count).map(|_| (0.0, Vec::new())).collect();
    if shards.is_empty() {
        return shards;
    }

    for (path, _key, weight) in weighted {
        let mut best = 0;
        for idx in 1..shards.len() {
            if shards[idx].0 < shards[best].0
                || (shards[idx].0 == shards[best].0 && shards[idx].1.len() < shards[best].1.len())
            {
                best = idx;
            }
        }
        shards[best].0 += weight;
        shards[best].1.push(path);
    }

    shards
}
