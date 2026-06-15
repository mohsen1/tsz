use crate::cli::{Args, ShardStrategy};
use crate::test_filter::{is_conformance_source_file, matches_path_filter};
use crate::test_parser::{parse_test_file, should_skip_test};
use crate::text_decode::{decode_source_text, DecodedSourceText};
use anyhow::Context;
use serde::Serialize;
use std::collections::HashSet;
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
    pub total: usize,
    pub passed: usize,
    pub weight: usize,
    pub shards: Vec<ConformanceShardPlanEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConformanceShardPlanEntry {
    pub index: usize,
    pub total: usize,
    pub passed: usize,
    pub weight: usize,
}

#[derive(Debug, Default)]
struct ShardAccumulator {
    total: usize,
    passed: usize,
    weight: f64,
}

impl ShardAccumulator {
    fn add(&mut self, passed: bool, weight: f64) {
        self.total += 1;
        self.passed += usize::from(passed);
        self.weight += weight;
    }

    fn entry(&self, index: usize) -> ConformanceShardPlanEntry {
        ConformanceShardPlanEntry {
            index,
            total: self.total,
            passed: self.passed,
            weight: integer_weight(self.weight),
        }
    }
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
    // A skipped test is reported as SKIP at runtime, never as a PASS, so it
    // contributes to a shard's `total` (matching the runner's coverage count)
    // but never to `passed`, regardless of any stale baseline `PASS` entry.
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
                let passed = plan_path_passes(&baseline_passes, &path, test_dir)?;
                shards[index].add(passed, weight);
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
                    let passed = plan_path_passes(&baseline_passes, &path, test_dir)?;
                    shards[index].add(passed, weight);
                }
            }
        }
    }

    let total = shards.iter().map(|shard| shard.total).sum();
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
        total,
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
// byte-for-byte (#13397). Skip status is applied later: at plan time it zeroes
// a test's pass contribution (`plan_path_passes`), and at run time the runner
// reports the test as SKIP. It never removes a test from the partition, because
// dropping members would shift the weighted bin-packing of the remaining tests.
fn discover_candidate_tests(args: &Args) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(&args.test_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if path.is_dir() || is_appledouble_file(path) {
            continue;
        }
        if !is_conformance_source_file(path) {
            continue;
        }
        if !matches_path_filter(path, args.filter.as_deref()) {
            continue;
        }
        files.push(path.to_path_buf());
    }

    files.sort();
    Ok(files)
}

fn has_skip_directive(path: &Path) -> anyhow::Result<bool> {
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(false);
    };
    let content = match decode_source_text(&bytes) {
        DecodedSourceText::Text(content) | DecodedSourceText::TextWithOriginalBytes(content, _) => {
            content
        }
        DecodedSourceText::Binary(_) => return Ok(false),
    };
    let parsed = parse_test_file(&content)
        .with_context(|| format!("failed to parse test directives in {}", path.display()))?;
    Ok(should_skip_test(&parsed.directives).is_some())
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

// A test counts toward a shard's planned `passed` only when the baseline
// records it as a PASS and it is not skipped. A `@skip` test is reported as
// SKIP at run time, never PASS, so it must not be counted as a planned pass
// even if a stale baseline entry marks it PASS — that would let the planner's
// expected-pass total drift above what the runner can ever report.
fn plan_path_passes(
    baseline_passes: &HashSet<String>,
    path: &Path,
    test_dir: &Path,
) -> anyhow::Result<bool> {
    if !is_baseline_pass(baseline_passes, path, test_dir) {
        return Ok(false);
    }
    Ok(!has_skip_directive(path)?)
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

    #[test]
    fn plan_includes_skipped_tests_but_never_counts_them_as_passed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cases = temp.path().join("TypeScript/tests/cases");
        write(&cases.join("compiler/pass.ts"), "let pass = 1;\n");
        write(&cases.join("compiler/fail.js"), "let fail = 1;\n");
        write(
            &cases.join("compiler/skipped.ts"),
            "// @skip: tracked upstream\n",
        );
        write(
            &cases.join("compiler/lib.d.ts"),
            "declare const ignored: string;\n",
        );
        write(&cases.join("fourslash/quickInfo.ts"), "ignored\n");
        let baseline = temp.path().join("baseline.txt");
        // skipped.ts carries a stale baseline PASS to prove the planner never
        // counts a skipped test as passed (it is SKIP at run time).
        write(
            &baseline,
            "PASS TypeScript/tests/cases/compiler/pass.ts\n\
             FAIL TypeScript/tests/cases/compiler/fail.js\n\
             PASS TypeScript/tests/cases/compiler/skipped.ts\n",
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
        // pass.ts, fail.js, and skipped.ts are all conformance candidates; the
        // skipped test stays in the partition so the planner's membership
        // matches the runner's `discover_tests` set (which includes skips).
        assert_eq!(plan.total, 3);
        // Only pass.ts counts as a planned pass; the skipped test's stale
        // baseline PASS is ignored.
        assert_eq!(plan.passed, 1);
        assert_eq!(
            plan.shards.iter().map(|shard| shard.total).sum::<usize>(),
            3
        );
        assert_eq!(
            plan.shards.iter().map(|shard| shard.passed).sum::<usize>(),
            1
        );
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
            // The planner counts the same number of tests in this shard as the
            // runner actually places there.
            assert_eq!(
                plan.shards[shard_index].total,
                members.len(),
                "plan and runner disagree on shard {shard_index} membership count"
            );
        }
        // No test is double-counted or dropped across shards.
        assert_eq!(runner_total, plan.total);
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
        assert_eq!(plan.total, 3);
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
