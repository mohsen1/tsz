use crate::cli::{Args, ShardStrategy};
use crate::test_filter::{is_conformance_source_file, matches_path_filter};
use crate::test_parser::{
    parse_test_file, select_ts7_oracle_configurations, should_skip_test_at_path,
};
use crate::text_decode::{decode_source_text, DecodedSourceText};
use anyhow::Context;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::runner_helpers::{
    estimated_test_weight, is_appledouble_file, load_json_weights, normalized_path,
    stable_shard_for_path, weighted_shard_files, weighted_shards,
};

const DEFAULT_BASELINE_PATH: &str = "scripts/conformance/conformance-baseline.txt";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConformanceShardPlan {
    pub strategy: String,
    pub shard_count: usize,
    pub candidates: usize,
    /// Backward-compatible runnable denominator.
    pub total: usize,
    pub runnable: usize,
    pub unsupported: usize,
    pub skipped: usize,
    pub passed: usize,
    pub weight: usize,
    pub shards: Vec<ConformanceShardPlanEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConformanceShardPlanEntry {
    pub index: usize,
    pub candidates: usize,
    /// Backward-compatible runnable denominator.
    pub total: usize,
    pub runnable: usize,
    pub unsupported: usize,
    pub skipped: usize,
    pub passed: usize,
    pub weight: usize,
}

#[derive(Debug, Default)]
struct ShardAccumulator {
    candidates: usize,
    runnable: usize,
    unsupported: usize,
    skipped: usize,
    passed: usize,
    weight: f64,
}

impl ShardAccumulator {
    fn add(&mut self, disposition: PlanDisposition, passed: bool, weight: f64) {
        self.candidates += 1;
        match disposition {
            PlanDisposition::Runnable => {
                self.runnable += 1;
                self.passed += usize::from(passed);
            }
            PlanDisposition::Unsupported => self.unsupported += 1,
            PlanDisposition::Skipped => self.skipped += 1,
        }
        self.weight += weight;
    }

    fn entry(&self, index: usize) -> ConformanceShardPlanEntry {
        debug_assert_eq!(
            self.candidates,
            self.runnable + self.unsupported + self.skipped
        );
        debug_assert!(self.passed <= self.runnable);
        ConformanceShardPlanEntry {
            index,
            candidates: self.candidates,
            total: self.runnable,
            runnable: self.runnable,
            unsupported: self.unsupported,
            skipped: self.skipped,
            passed: self.passed,
            weight: integer_weight(self.weight),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanDisposition {
    Runnable,
    Unsupported,
    Skipped,
}

pub fn build_shard_plan(args: &Args, shard_count: usize) -> anyhow::Result<ConformanceShardPlan> {
    build_shard_plan_with_baseline(args, shard_count, Path::new(DEFAULT_BASELINE_PATH))
}

fn build_shard_plan_with_baseline(
    args: &Args,
    shard_count: usize,
    baseline_path: &Path,
) -> anyhow::Result<ConformanceShardPlan> {
    if shard_count == 0 {
        anyhow::bail!("--plan count must be greater than zero");
    }

    // Plan over the exact candidate set the runner partitions. The runner's
    // membership source (`discover_tests`) includes skipped tests, so the
    // planner must too. The weighted strategy's greedy bin-packing depends on
    // the full input set: dropping skipped tests here would place real tests in
    // different shards than the runner, so the planner and runner would disagree
    // on per-shard membership and a real test could be double-counted or dropped
    // (#13397, option 3). Using the identical candidate set makes the plan's
    // per-shard membership byte-identical to the runner's for a given checkout.
    //
    // Skipped and unsupported tests retain candidate membership and weight, but
    // contribute to neither the runnable denominator nor `passed`, regardless
    // of any stale baseline `PASS` entry.
    let files = discover_candidate_tests(args)?;
    let test_dir = Path::new(&args.test_dir);
    let baseline_passes = load_baseline_passes(baseline_path)?;
    let weights = args
        .shard_weights
        .as_deref()
        .and_then(|path| load_json_weights(Path::new(path)));
    let mut shards: Vec<ShardAccumulator> = (0..shard_count)
        .map(|_| ShardAccumulator::default())
        .collect();

    match args.shard_strategy {
        ShardStrategy::Hash => {
            for path in files {
                let index = stable_shard_for_path(&path, test_dir, shard_count);
                let weight = estimated_test_weight(weights.as_ref(), &path, test_dir);
                let disposition = plan_path_disposition(&path)?;
                let passed = disposition == PlanDisposition::Runnable
                    && is_baseline_pass(&baseline_passes, &path, test_dir);
                shards[index].add(disposition, passed, weight);
            }
        }
        ShardStrategy::Weighted => {
            for (index, (_weight, paths)) in
                weighted_shards(files, test_dir, shard_count, weights.as_ref())
                    .into_iter()
                    .enumerate()
            {
                for path in paths {
                    let weight = estimated_test_weight(weights.as_ref(), &path, test_dir);
                    let disposition = plan_path_disposition(&path)?;
                    let passed = disposition == PlanDisposition::Runnable
                        && is_baseline_pass(&baseline_passes, &path, test_dir);
                    shards[index].add(disposition, passed, weight);
                }
            }
        }
    }

    let candidates = shards.iter().map(|shard| shard.candidates).sum();
    let runnable = shards.iter().map(|shard| shard.runnable).sum();
    let unsupported = shards.iter().map(|shard| shard.unsupported).sum();
    let skipped = shards.iter().map(|shard| shard.skipped).sum();
    debug_assert_eq!(candidates, runnable + unsupported + skipped);
    let passed = shards.iter().map(|shard| shard.passed).sum();
    let weight = integer_weight(shards.iter().map(|shard| shard.weight).sum());
    let shards = shards
        .iter()
        .enumerate()
        .map(|(index, shard)| shard.entry(index))
        .collect();

    Ok(ConformanceShardPlan {
        strategy: shard_strategy_name(&args.shard_strategy).to_string(),
        shard_count,
        candidates,
        total: runnable,
        runnable,
        unsupported,
        skipped,
        passed,
        weight,
        shards,
    })
}

pub(crate) fn discover_tests(args: &Args) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = discover_candidate_tests(args)?;

    if let Some((shard_index, shard_count)) = parse_shard_spec(args.shard.as_deref())? {
        let test_dir_path = Path::new(&args.test_dir);
        files = match args.shard_strategy {
            ShardStrategy::Hash => files
                .into_iter()
                .filter(|path| {
                    stable_shard_for_path(path, test_dir_path, shard_count) == shard_index
                })
                .collect(),
            ShardStrategy::Weighted => {
                let weights = args
                    .shard_weights
                    .as_deref()
                    .and_then(|path| load_json_weights(Path::new(path)));
                weighted_shard_files(
                    files,
                    test_dir_path,
                    shard_index,
                    shard_count,
                    weights.as_ref(),
                )
            }
        };
    }

    if args.offset > 0 {
        if args.offset >= files.len() {
            files.clear();
        } else {
            files = files.split_off(args.offset);
        }
    }

    if files.len() > args.max {
        files.truncate(args.max);
    }

    Ok(files)
}

pub(crate) fn parse_shard_spec(spec: Option<&str>) -> anyhow::Result<Option<(usize, usize)>> {
    let Some(spec) = spec else {
        return Ok(None);
    };
    let Some((index, count)) = spec.split_once('/') else {
        anyhow::bail!("--shard must be formatted as index/count, got {spec:?}");
    };
    let index = index
        .parse::<usize>()
        .with_context(|| format!("invalid --shard index in {spec:?}"))?;
    let count = count
        .parse::<usize>()
        .with_context(|| format!("invalid --shard count in {spec:?}"))?;
    if count == 0 {
        anyhow::bail!("--shard count must be greater than zero");
    }
    if index >= count {
        anyhow::bail!("--shard index {index} must be less than count {count}");
    }
    Ok(Some((index, count)))
}

// Discover the full candidate set (including tests with a `@skip` directive).
// Both the planner (`build_shard_plan`) and the runner membership source
// (`discover_tests`) consume this identical set so their partitions agree
// byte-for-byte (#13397). Runtime disposition is applied later and never removes
// a test from the partition, because dropping skipped or unsupported members
// would shift the weighted bin-packing of the remaining tests.
fn discover_candidate_tests(args: &Args) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = discover_all_candidate_tests(&args.test_dir)?;
    files.retain(|path| matches_path_filter(path, args.filter.as_deref()));
    Ok(files)
}

fn discover_all_candidate_tests(test_dir: &str) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(test_dir).follow_links(true) {
        let entry =
            entry.with_context(|| format!("failed to walk conformance corpus {test_dir}"))?;
        let path = entry.path();
        if path.is_dir() || is_appledouble_file(path) {
            continue;
        }
        if !is_conformance_source_file(path) {
            continue;
        }
        files.push(path.to_path_buf());
    }

    files.sort();
    Ok(files)
}

fn plan_path_disposition(path: &Path) -> anyhow::Result<PlanDisposition> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read conformance candidate {}", path.display()))?;
    let content = match decode_source_text(&bytes) {
        DecodedSourceText::Text(content) | DecodedSourceText::TextWithOriginalBytes(content, _) => {
            content
        }
        DecodedSourceText::Binary(_) => return Ok(PlanDisposition::Runnable),
    };
    let parsed = parse_test_file(&content)
        .with_context(|| format!("failed to parse test directives in {}", path.display()))?;
    let Some(reason) = should_skip_test_at_path(path, &parsed.directives) else {
        return Ok(PlanDisposition::Runnable);
    };
    if reason == "unsupported by TypeScript 7"
        && select_ts7_oracle_configurations(&parsed.directives).is_err()
    {
        return Ok(PlanDisposition::Unsupported);
    }
    Ok(PlanDisposition::Skipped)
}

fn disposition_identity(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read conformance candidate {}", path.display()))?;
    let content = match decode_source_text(&bytes) {
        DecodedSourceText::Text(content) | DecodedSourceText::TextWithOriginalBytes(content, _) => {
            content
        }
        DecodedSourceText::Binary(_) => return Ok("runnable".to_string()),
    };
    let parsed = parse_test_file(&content)
        .with_context(|| format!("failed to parse test directives in {}", path.display()))?;
    let Some(reason) = should_skip_test_at_path(path, &parsed.directives) else {
        return Ok("runnable".to_string());
    };
    if reason == "unsupported by TypeScript 7"
        && select_ts7_oracle_configurations(&parsed.directives).is_err()
    {
        return Ok(format!(
            "unsupported:{}",
            crate::tsc_results::UnsupportedReason::TypeScript7Configuration.code()
        ));
    }
    Ok(format!("skipped:{reason}"))
}

/// Bind live corpus discovery and selector classification exactly to the
/// cache/domain pair before applying any invocation subset.
pub(crate) fn validate_live_domain(
    args: &Args,
    cache: &crate::cache::TscCache,
    domain: &crate::cache::ConformanceDomain,
) -> anyhow::Result<BTreeMap<String, String>> {
    if domain.schema_version != 2
        || !crate::integrity::is_lower_hex(&domain.corpus_commit, 40)
        || !crate::integrity::is_lower_hex(&domain.corpus_tree, 40)
        || !crate::integrity::is_lower_hex(&domain.candidate_content_sha256, 64)
    {
        anyhow::bail!("cache/domain identity schema is incomplete");
    }
    let cache_paths = cache
        .keys()
        .map(|path| path.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    let unsupported_paths = domain
        .unsupported
        .keys()
        .map(|path| path.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    let skipped_paths = domain
        .skipped
        .keys()
        .map(|path| path.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    if cache_paths.len() != cache.len()
        || unsupported_paths.len() != domain.unsupported.len()
        || skipped_paths.len() != domain.skipped.len()
    {
        anyhow::bail!("cache/domain contains duplicate normalized paths");
    }
    if !cache_paths.is_disjoint(&unsupported_paths)
        || !cache_paths.is_disjoint(&skipped_paths)
        || !unsupported_paths.is_disjoint(&skipped_paths)
    {
        anyhow::bail!("cache/domain path classifications overlap");
    }

    let mut expected = BTreeMap::new();
    for path in cache.keys() {
        expected.insert(path.replace('\\', "/"), "runnable".to_string());
    }
    for (path, reason) in &domain.unsupported {
        expected.insert(path.replace('\\', "/"), format!("unsupported:{reason}"));
    }
    for (path, reason) in &domain.skipped {
        expected.insert(path.replace('\\', "/"), format!("skipped:{reason}"));
    }

    let declared_partition_count = domain
        .runnable_count
        .checked_add(domain.unsupported_count)
        .and_then(|count| count.checked_add(domain.skipped_count));
    if domain.runnable_count != cache.len()
        || domain.unsupported_count != domain.unsupported.len()
        || domain.skipped_count != domain.skipped.len()
        || declared_partition_count != Some(domain.candidate_count)
        || domain.candidate_count != expected.len()
    {
        anyhow::bail!("cache/domain declared partition counts are inconsistent");
    }
    if cache.values().any(|entry| {
        entry.metadata.typescript_version.as_deref() != Some(domain.typescript_version.as_str())
    }) {
        anyhow::bail!("cache/domain TypeScript version identity is inconsistent");
    }

    let test_dir = Path::new(&args.test_dir);
    let mut live = BTreeMap::new();
    let mut source_hashes = BTreeMap::new();
    let mut content_records = Vec::new();
    for path in discover_all_candidate_tests(&args.test_dir)? {
        let key = crate::cache::cache_key(&path, test_dir)
            .with_context(|| format!("candidate escaped test root: {}", path.display()))?
            .replace('\\', "/");
        let disposition = disposition_identity(&path)?;
        let source_sha256 = crate::integrity::sha256_bytes(
            &std::fs::read(&path)
                .with_context(|| format!("failed to hash candidate {}", path.display()))?,
        );
        if disposition == "runnable" {
            let cached = cache
                .get(&key)
                .with_context(|| format!("runnable candidate has no cache row: {key}"))?;
            if cached.metadata.source_sha256 != source_sha256 {
                anyhow::bail!("runnable candidate source hash differs from cache: {key}");
            }
        }
        if live.insert(key.clone(), disposition.clone()).is_some() {
            anyhow::bail!("live conformance corpus contains duplicate path identity {key}");
        }
        source_hashes.insert(key.clone(), source_sha256.clone());
        content_records.push((key, disposition, source_sha256));
    }
    if live != expected {
        let first = live
            .iter()
            .find(|(path, identity)| expected.get(*path) != Some(*identity))
            .map(|(path, identity)| format!("live {path}={identity}"))
            .or_else(|| {
                expected
                    .keys()
                    .find(|path| !live.contains_key(*path))
                    .map(|path| format!("missing live {path}"))
            })
            .unwrap_or_else(|| "classification mismatch".to_string());
        anyhow::bail!("live conformance domain differs from cache/domain: {first}");
    }
    let live_content_sha256 = crate::integrity::candidate_content_sha256(&content_records);
    if live_content_sha256 != domain.candidate_content_sha256 {
        anyhow::bail!("live conformance candidate bytes differ from cache/domain");
    }
    Ok(source_hashes)
}

fn load_baseline_passes(path: &Path) -> anyhow::Result<HashSet<String>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read conformance baseline {}", path.display()))?;
    let mut passes = HashSet::new();
    for line in content.lines() {
        let Some((status, rest)) = line.split_once(' ') else {
            continue;
        };
        if status != "PASS" {
            continue;
        }
        let path = rest.split(" | ").next().unwrap_or(rest).replace('\\', "/");
        passes.insert(path);
    }
    Ok(passes)
}

fn is_baseline_pass(baseline_passes: &HashSet<String>, path: &Path, test_dir: &Path) -> bool {
    baseline_keys(path, test_dir)
        .into_iter()
        .any(|key| baseline_passes.contains(&key))
}

fn baseline_keys(path: &Path, test_dir: &Path) -> Vec<String> {
    let full = normalized_path(path);
    let rel = path
        .strip_prefix(test_dir)
        .map(normalized_path)
        .unwrap_or_else(|_| full.clone());
    vec![full, rel.clone(), format!("TypeScript/tests/cases/{rel}")]
}

fn integer_weight(weight: f64) -> usize {
    if weight.is_finite() && weight > 0.0 {
        weight as usize
    } else {
        0
    }
}

fn shard_strategy_name(strategy: &ShardStrategy) -> &'static str {
    match strategy {
        ShardStrategy::Hash => "hash",
        ShardStrategy::Weighted => "weighted",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse_args(input: &[&str]) -> Args {
        Args::try_parse_from(input).expect("argument parsing should succeed in test")
    }

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent directory should be created");
        }
        std::fs::write(path, content).expect("test file should be written");
    }

    fn cache_result(source: &[u8]) -> crate::tsc_results::TscResult {
        crate::tsc_results::TscResult {
            metadata: crate::tsc_results::FileMetadata {
                mtime_ms: 0,
                size: 0,
                typescript_version: Some("7.0.2".to_string()),
                source_sha256: crate::integrity::sha256_bytes(source),
            },
            error_codes: Vec::new(),
            diagnostic_fingerprints: Vec::new(),
            diagnostic_blocks_complete: true,
            ordinary_exit_statuses: vec![0],
        }
    }

    #[test]
    fn live_domain_requires_exact_paths_and_classifications() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cases = temp.path().join("cases");
        write(&cases.join("compiler/run.ts"), "let value = 1;\n");
        write(&cases.join("compiler/skip.ts"), "// @skip: tracked\n");
        write(
            &cases.join("compiler/unsupported.ts"),
            "// @target: es5\nlet value = 1;\n",
        );
        let args = parse_args(&[
            "tsz-conformance",
            "--test-dir",
            cases.to_str().expect("utf8 path"),
        ]);
        let run_path = cases.join("compiler/run.ts");
        let cache = crate::cache::TscCache::from([(
            "compiler/run.ts".to_string(),
            cache_result(&std::fs::read(&run_path).expect("run source")),
        )]);
        let skip_path = cases.join("compiler/skip.ts");
        let unsupported_path = cases.join("compiler/unsupported.ts");
        let records = [
            (&run_path, "compiler/run.ts"),
            (&skip_path, "compiler/skip.ts"),
            (&unsupported_path, "compiler/unsupported.ts"),
        ]
        .into_iter()
        .map(|(path, key)| {
            let bytes = std::fs::read(path).expect("candidate source");
            (
                key.to_string(),
                disposition_identity(path).expect("disposition"),
                crate::integrity::sha256_bytes(&bytes),
            )
        })
        .collect::<Vec<_>>();
        let mut domain = crate::cache::ConformanceDomain {
            schema_version: 2,
            typescript_version: "7.0.2".to_string(),
            corpus_commit: "0".repeat(40),
            corpus_tree: "1".repeat(40),
            candidate_content_sha256: crate::integrity::candidate_content_sha256(&records),
            oracle: serde_json::json!({}),
            candidate_count: 3,
            runnable_count: 1,
            unsupported_count: 1,
            skipped_count: 1,
            unsupported: BTreeMap::from([(
                "compiler/unsupported.ts".to_string(),
                crate::tsc_results::UnsupportedReason::TypeScript7Configuration
                    .code()
                    .to_string(),
            )]),
            skipped: BTreeMap::from([("compiler/skip.ts".to_string(), "@skip".to_string())]),
        };

        validate_live_domain(&args, &cache, &domain).expect("exact live domain");
        domain
            .skipped
            .insert("compiler/run.ts".to_string(), "overlap".to_string());
        domain.skipped_count += 1;
        domain.candidate_count += 1;
        assert!(validate_live_domain(&args, &cache, &domain).is_err());
        domain.skipped.remove("compiler/run.ts");
        domain.skipped_count -= 1;
        domain.candidate_count -= 1;
        domain
            .skipped
            .insert("compiler/skip.ts".to_string(), "different".to_string());
        assert!(validate_live_domain(&args, &cache, &domain).is_err());

        // Same path and selector classification with different raw bytes must
        // fail both the per-row source identity and aggregate candidate digest.
        domain
            .skipped
            .insert("compiler/skip.ts".to_string(), "@skip".to_string());
        write(&run_path, "let value = 2;\n");
        assert!(validate_live_domain(&args, &cache, &domain).is_err());
    }

    #[test]
    fn plan_keeps_non_runnable_candidates_out_of_total_and_passed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cases = temp.path().join("TypeScript/tests/cases");
        write(&cases.join("compiler/pass.ts"), "let pass = 1;\n");
        write(&cases.join("compiler/fail.js"), "let fail = 1;\n");
        write(
            &cases.join("compiler/skipped.ts"),
            "// @skip: tracked upstream\n",
        );
        write(
            &cases.join("compiler/unsupported.ts"),
            "// @target: es5\nlet unsupported = 1;\n",
        );
        write(
            &cases.join("compiler/lib.d.ts"),
            "declare const ignored: string;\n",
        );
        write(&cases.join("fourslash/quickInfo.ts"), "ignored\n");
        let baseline = temp.path().join("baseline.txt");
        // Both non-runnable rows carry stale baseline PASS entries to prove the
        // planner never counts them as passed.
        write(
            &baseline,
            "PASS TypeScript/tests/cases/compiler/pass.ts\n\
             FAIL TypeScript/tests/cases/compiler/fail.js\n\
             PASS TypeScript/tests/cases/compiler/skipped.ts\n\
             PASS TypeScript/tests/cases/compiler/unsupported.ts\n",
        );

        let args = parse_args(&[
            "tsz-conformance",
            "--plan",
            "2",
            "--test-dir",
            cases.to_str().unwrap(),
        ]);
        let plan = build_shard_plan_with_baseline(&args, 2, &baseline).unwrap();

        assert_eq!(plan.shard_count, 2);
        assert_eq!(plan.candidates, 4);
        assert_eq!(plan.total, 2);
        assert_eq!(plan.runnable, 2);
        assert_eq!(plan.unsupported, 1);
        assert_eq!(plan.skipped, 1);
        assert_eq!(
            plan.candidates,
            plan.runnable + plan.unsupported + plan.skipped
        );
        // Only pass.ts counts as a planned pass; both stale non-runnable PASS
        // entries are ignored.
        assert_eq!(plan.passed, 1);
        assert_eq!(
            plan.shards
                .iter()
                .map(|shard| shard.candidates)
                .sum::<usize>(),
            4
        );
        assert_eq!(
            plan.shards.iter().map(|shard| shard.total).sum::<usize>(),
            2
        );
        assert_eq!(
            plan.shards.iter().map(|shard| shard.passed).sum::<usize>(),
            1
        );
        for shard in &plan.shards {
            assert_eq!(shard.total, shard.runnable);
            assert_eq!(
                shard.candidates,
                shard.runnable + shard.unsupported + shard.skipped
            );
        }
    }

    /// The planner's per-shard membership must equal the runner's per-shard
    /// membership for every shard, including under the weighted strategy whose
    /// greedy bin-packing is sensitive to the candidate set. Otherwise a real
    /// test counted by the plan could run on a different shard (or be dropped),
    /// which is the within-run divergence #13397 calls out.
    #[test]
    fn plan_and_runner_agree_on_per_shard_membership_weighted() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cases = temp.path().join("TypeScript/tests/cases");
        for i in 0..12 {
            write(
                &cases.join(format!("compiler/t{i}.ts")),
                &format!("let t{i} = 1;\n"),
            );
        }
        // A couple of skipped tests with non-trivial weight: they must occupy a
        // shard slot in both the plan and the runner so neither side reweights
        // the bin-packing of the real tests differently.
        write(
            &cases.join("compiler/skip_a.ts"),
            "// @skip: tracked\nlet skip_a = 1;\n",
        );
        write(
            &cases.join("compiler/skip_b.ts"),
            "// @skip: tracked\nlet skip_b = 1;\n",
        );
        let baseline = temp.path().join("baseline.txt");
        write(&baseline, "");
        let weights = temp.path().join("weights.json");
        write(
            &weights,
            &serde_json::json!({
                "path_weights": {
                    "TypeScript/tests/cases/compiler/t0.ts": 9.0,
                    "TypeScript/tests/cases/compiler/t1.ts": 8.0,
                    "TypeScript/tests/cases/compiler/t2.ts": 7.0,
                    "TypeScript/tests/cases/compiler/t3.ts": 6.0,
                    "TypeScript/tests/cases/compiler/skip_a.ts": 5.0,
                    "TypeScript/tests/cases/compiler/skip_b.ts": 4.0
                }
            })
            .to_string(),
        );

        let shard_count = 4usize;
        let plan_args = parse_args(&[
            "tsz-conformance",
            "--plan",
            "4",
            "--test-dir",
            cases.to_str().unwrap(),
            "--shard-strategy",
            "weighted",
            "--shard-weights",
            weights.to_str().unwrap(),
        ]);
        let plan = build_shard_plan_with_baseline(&plan_args, shard_count, &baseline).unwrap();

        let mut runner_total = 0usize;
        for shard_index in 0..shard_count {
            let runner_args = parse_args(&[
                "tsz-conformance",
                "--shard",
                &format!("{shard_index}/{shard_count}"),
                "--test-dir",
                cases.to_str().unwrap(),
                "--shard-strategy",
                "weighted",
                "--shard-weights",
                weights.to_str().unwrap(),
            ]);
            let members = discover_tests(&runner_args).unwrap();
            runner_total += members.len();
            // Candidate membership remains byte-identical even though the
            // runnable denominator excludes skips.
            assert_eq!(
                plan.shards[shard_index].candidates,
                members.len(),
                "plan and runner disagree on shard {shard_index} membership count"
            );
        }
        // No test is double-counted or dropped across shards.
        assert_eq!(runner_total, plan.candidates);
        assert_eq!(plan.candidates, 14);
        assert_eq!(plan.total, 12);
        assert_eq!(plan.runnable, 12);
        assert_eq!(plan.unsupported, 0);
        assert_eq!(plan.skipped, 2);
    }

    /// Two plan builds with identical inputs must be byte-identical: the
    /// per-SHA shard partition cannot depend on wall-clock or mutable state.
    #[test]
    fn plan_is_deterministic_for_fixed_inputs() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cases = temp.path().join("TypeScript/tests/cases");
        for i in 0..20 {
            write(
                &cases.join(format!("compiler/case{i}.ts")),
                &format!("let case{i} = {i};\n"),
            );
        }
        let baseline = temp.path().join("baseline.txt");
        write(&baseline, "PASS TypeScript/tests/cases/compiler/case0.ts\n");
        let weights = temp.path().join("weights.json");
        write(
            &weights,
            &serde_json::json!({
                "hash_bucket_weights": { "shard_count": 4, "weights": [9.0, 7.0, 5.0, 3.0] }
            })
            .to_string(),
        );

        let args = parse_args(&[
            "tsz-conformance",
            "--plan",
            "4",
            "--test-dir",
            cases.to_str().unwrap(),
            "--shard-strategy",
            "weighted",
            "--shard-weights",
            weights.to_str().unwrap(),
        ]);
        let first = build_shard_plan_with_baseline(&args, 4, &baseline).unwrap();
        let second = build_shard_plan_with_baseline(&args, 4, &baseline).unwrap();
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap(),
            "shard plan must be byte-identical for identical inputs"
        );
    }

    #[test]
    fn weighted_plan_uses_historical_weights_for_all_shards() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cases = temp.path().join("TypeScript/tests/cases");
        write(&cases.join("compiler/a.ts"), "let a = 1;\n");
        write(&cases.join("compiler/b.ts"), "let b = 1;\n");
        write(&cases.join("compiler/c.ts"), "let c = 1;\n");
        let baseline = temp.path().join("baseline.txt");
        write(
            &baseline,
            "PASS TypeScript/tests/cases/compiler/a.ts\n\
             PASS TypeScript/tests/cases/compiler/b.ts\n\
             PASS TypeScript/tests/cases/compiler/c.ts\n",
        );
        let weights = temp.path().join("weights.json");
        write(
            &weights,
            &serde_json::json!({
                "path_weights": {
                    "TypeScript/tests/cases/compiler/a.ts": 8.0,
                    "TypeScript/tests/cases/compiler/b.ts": 5.0,
                    "TypeScript/tests/cases/compiler/c.ts": 3.0
                }
            })
            .to_string(),
        );

        let args = parse_args(&[
            "tsz-conformance",
            "--plan",
            "2",
            "--test-dir",
            cases.to_str().unwrap(),
            "--shard-strategy",
            "weighted",
            "--shard-weights",
            weights.to_str().unwrap(),
        ]);
        let plan = build_shard_plan_with_baseline(&args, 2, &baseline).unwrap();

        assert_eq!(plan.strategy, "weighted");
        assert_eq!(plan.candidates, 3);
        assert_eq!(plan.total, 3);
        assert_eq!(plan.runnable, 3);
        assert_eq!(plan.unsupported, 0);
        assert_eq!(plan.skipped, 0);
        assert_eq!(plan.passed, 3);
        assert_eq!(
            plan.shards
                .iter()
                .map(|shard| shard.weight)
                .collect::<Vec<_>>(),
            vec![8, 8]
        );
    }

    #[test]
    fn parse_shard_spec_rejects_out_of_range_index() {
        let err = parse_shard_spec(Some("2/2")).unwrap_err().to_string();
        assert!(err.contains("must be less than count"));
    }
}
