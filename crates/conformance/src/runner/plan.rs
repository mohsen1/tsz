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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkipPolicy {
    IncludeSkipped,
    ExcludeSkipped,
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

    let files = discover_candidate_tests(args, SkipPolicy::ExcludeSkipped)?;
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
                let passed = is_baseline_pass(&baseline_passes, &path, test_dir);
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
                    let passed = is_baseline_pass(&baseline_passes, &path, test_dir);
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
    let mut files = discover_candidate_tests(args, SkipPolicy::IncludeSkipped)?;

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

fn discover_candidate_tests(args: &Args, skip_policy: SkipPolicy) -> anyhow::Result<Vec<PathBuf>> {
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
        if skip_policy == SkipPolicy::ExcludeSkipped && has_skip_directive(path)? {
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
    fn plan_excludes_skipped_tests_and_counts_baseline_passes() {
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
        assert_eq!(plan.total, 2);
        assert_eq!(plan.passed, 1);
        assert_eq!(
            plan.shards.iter().map(|shard| shard.total).sum::<usize>(),
            2
        );
        assert_eq!(
            plan.shards.iter().map(|shard| shard.passed).sum::<usize>(),
            1
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
