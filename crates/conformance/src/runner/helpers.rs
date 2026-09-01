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

pub(super) fn sanitize_artifact_name(path: &str) -> String {
    path.chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect()
}

/// Return cached oracle facts without filtering, sorting, or rewriting them.
pub(super) fn canonical_tsc_diagnostics(
    tsc_result: &crate::tsc_results::TscResult,
) -> (Vec<u32>, Vec<DiagnosticFingerprint>, Vec<u8>) {
    (
        tsc_result.error_codes.clone(),
        tsc_result.diagnostic_fingerprints.clone(),
        tsc_result.ordinary_exit_statuses.clone(),
    )
}

/// Compare canonical TSZ and TypeScript diagnostics and produce a `TestResult`.
///
/// Top-level order, grouped-block ownership, multiplicity, and ordinary exits
/// are all observable compiler behavior. Nothing is sorted or elected.
///
/// `options` is threaded into `TestResult::Fail` unchanged — callers that
/// intentionally drop the options map (e.g. the UTF-16 path) can pass
/// `HashMap::new()`.
pub(super) fn compare_diagnostics(
    compile_result: &tsz_wrapper::CompilationResult,
    tsc_error_codes: &[u32],
    tsc_fps: &[DiagnosticFingerprint],
    tsc_exit_statuses: &[u8],
    options: HashMap<String, String>,
) -> TestResult {
    fn multiset_difference<T>(left: &[T], right: &[T]) -> Vec<T>
    where
        T: Clone + Eq + std::hash::Hash,
    {
        let mut remaining: std::collections::HashMap<&T, usize> = std::collections::HashMap::new();
        for value in right {
            *remaining.entry(value).or_default() += 1;
        }
        left.iter()
            .filter_map(|value| match remaining.get_mut(value) {
                Some(count) if *count > 0 => {
                    *count -= 1;
                    None
                }
                _ => Some(value.clone()),
            })
            .collect()
    }

    let missing = multiset_difference(tsc_error_codes, &compile_result.error_codes);
    let extra = multiset_difference(&compile_result.error_codes, tsc_error_codes);

    let missing_fingerprints =
        multiset_difference(tsc_fps, &compile_result.diagnostic_fingerprints);
    let extra_fingerprints = multiset_difference(&compile_result.diagnostic_fingerprints, tsc_fps);
    let expected_fingerprint_codes = tsc_fps.iter().map(|item| item.code).collect::<Vec<_>>();
    let actual_fingerprint_codes = compile_result
        .diagnostic_fingerprints
        .iter()
        .map(|item| item.code)
        .collect::<Vec<_>>();
    let exact = tsc_error_codes == compile_result.error_codes
        && tsc_fps == compile_result.diagnostic_fingerprints
        && tsc_error_codes == expected_fingerprint_codes
        && compile_result.error_codes == actual_fingerprint_codes
        && tsc_exit_statuses == compile_result.ordinary_exit_statuses;
    if exact {
        TestResult::Pass
    } else {
        TestResult::Fail(Box::new(TestResultFail {
            expected: tsc_error_codes.to_vec(),
            actual: compile_result.error_codes.clone(),
            missing,
            extra,
            missing_fingerprints,
            extra_fingerprints,
            expected_fingerprints: tsc_fps.to_vec(),
            actual_fingerprints: compile_result.diagnostic_fingerprints.clone(),
            expected_exit_statuses: tsc_exit_statuses.to_vec(),
            actual_exit_statuses: compile_result.ordinary_exit_statuses.clone(),
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
