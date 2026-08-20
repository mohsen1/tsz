#!/usr/bin/env node

import fs from "node:fs";
import process from "node:process";
import { fileURLToPath } from "node:url";

export const KNOWN_UNIT_CRATES = [
  "tsz-core",
  "tsz-cli",
  "tsz-conformance",
];

const DOCS_ONLY_PATTERN =
  /(^|\/)[^/]+\.md$|^(LICENSE|CHANGELOG|CONTRIBUTING|CODE_OF_CONDUCT)([.-][^/]*)?$|^docs\/|^\.gitignore$|^\.editorconfig$/;

const COMPILER_PATH_PATTERN =
  /^(Cargo\.(lock|toml)|\.cargo\/|rust-toolchain|\.github\/workflows\/(ci|bench)\.yml|crates\/clippy\.toml|crates\/(conformance|tsz-(cli|core))(\/|$)|benches\/|tests\/|TypeScript\/|scripts\/(conformance|emit|fourslash|tsc|dts|snapshot)|scripts\/ci\/gate-path-classifier\.mjs|scripts\/ci\/lib\/[^/]+\.sh|scripts\/ci\/(ci-resources|full-ci|github-suite|suite-metadata|build-dist|dist)[^/]*\.sh)/;

const BENCH_SHELL_PATTERN = /^scripts\/bench\/[^/]+\.sh$/;
const PERF_TOOL_PATTERN = /^scripts\/perf\/[^/]+\.(py|sh)$/;
const ARCH_TOOL_PATTERN = /^scripts\/arch\/[^/]+\.py$/;
const CACHE_KEY_INPUT_PATTERN = /^(Cargo\.lock|Cargo\.toml|\.cargo\/config\.toml)$/;

const DRAFT_BLAST_PATTERN =
  /^(Cargo\.(lock|toml)|\.cargo\/|rust-toolchain|scripts\/ci\/|\.github\/workflows\/|crates\/[^/]+\/(Cargo\.toml|build\.rs))/;
const CRATE_SOURCE_PATTERN = /^crates\/[^/]+\/(src|tests|benches|examples)\//;

function sortedUnique(values) {
  return [...new Set(values.filter((value) => value.length > 0))].sort();
}

function crateNameForSourcePath(path) {
  const match = /^crates\/([^/]+)\//.exec(path);
  return match?.[1] ?? "";
}

function classifyDraftUnitNarrowing(paths) {
  const blastPaths = paths.filter((path) => DRAFT_BLAST_PATTERN.test(path));
  const cratePaths = paths.filter((path) => CRATE_SOURCE_PATTERN.test(path));
  const otherPaths = paths.filter((path) => (
    !DOCS_ONLY_PATTERN.test(path)
      && !CRATE_SOURCE_PATTERN.test(path)
      && !DRAFT_BLAST_PATTERN.test(path)
  ));
  const touchedCrates = sortedUnique(cratePaths.map(crateNameForSourcePath));
  const knownCrates = new Set(KNOWN_UNIT_CRATES);
  const unknownCrates = touchedCrates.filter((crate) => !knownCrates.has(crate));
  const canNarrow = blastPaths.length === 0
    && otherPaths.length === 0
    && cratePaths.length > 0
    && unknownCrates.length === 0;

  const reasons = [];
  if (blastPaths.length > 0) reasons.push("blast-radius paths touched");
  if (otherPaths.length > 0) reasons.push("unclassified paths touched");
  if (unknownCrates.length > 0) reasons.push("non-unit crate paths touched");
  if (cratePaths.length === 0 && reasons.length === 0) reasons.push("no crate source changes");

  return {
    canNarrow,
    unitPackages: canNarrow ? touchedCrates : [],
    touchedCrates,
    unknownCrates,
    blastPaths: sortedUnique(blastPaths),
    cratePaths: sortedUnique(cratePaths),
    otherPaths: sortedUnique(otherPaths),
    reason: reasons.join("; "),
  };
}

export function normalizePathList(input) {
  if (Array.isArray(input)) {
    return sortedUnique(input.map((path) => String(path)));
  }
  return sortedUnique(String(input).split(/\r?\n/));
}

export function classifyGatePaths(input) {
  const paths = normalizePathList(input);
  const nonDocsPaths = paths.filter((path) => !DOCS_ONLY_PATTERN.test(path));
  const compilerPaths = paths.filter((path) => COMPILER_PATH_PATTERN.test(path));
  const benchShellPaths = paths.filter((path) => BENCH_SHELL_PATTERN.test(path));
  const nonBenchShellPaths = paths.filter((path) => (
    !DOCS_ONLY_PATTERN.test(path) && !BENCH_SHELL_PATTERN.test(path)
  ));
  const perfToolPaths = paths.filter((path) => PERF_TOOL_PATTERN.test(path));
  const nonPerfToolPaths = paths.filter((path) => (
    !DOCS_ONLY_PATTERN.test(path) && !PERF_TOOL_PATTERN.test(path)
  ));
  const archToolPaths = paths.filter((path) => ARCH_TOOL_PATTERN.test(path));
  const nonArchToolPaths = paths.filter((path) => (
    !DOCS_ONLY_PATTERN.test(path) && !ARCH_TOOL_PATTERN.test(path)
  ));

  return {
    paths,
    docsOnly: paths.length > 0 && nonDocsPaths.length === 0,
    nonDocsPaths: sortedUnique(nonDocsPaths),
    compilerChecksRequired: compilerPaths.length > 0,
    compilerPaths: sortedUnique(compilerPaths),
    benchShellOnly: benchShellPaths.length > 0 && nonBenchShellPaths.length === 0,
    benchShellPaths: sortedUnique(benchShellPaths),
    nonBenchShellPaths: sortedUnique(nonBenchShellPaths),
    perfToolOnly: perfToolPaths.length > 0 && nonPerfToolPaths.length === 0,
    perfToolPaths: sortedUnique(perfToolPaths),
    nonPerfToolPaths: sortedUnique(nonPerfToolPaths),
    archToolOnly: archToolPaths.length > 0 && nonArchToolPaths.length === 0,
    archToolPaths: sortedUnique(archToolPaths),
    nonArchToolPaths: sortedUnique(nonArchToolPaths),
    cacheKeyInputPaths: sortedUnique(paths.filter((path) => CACHE_KEY_INPUT_PATTERN.test(path))),
    draftUnitNarrow: classifyDraftUnitNarrowing(paths),
  };
}

function usage() {
  return `usage: node scripts/ci/gate-path-classifier.mjs [--file PATH]\n\nReads newline-delimited paths from stdin by default and writes JSON.`;
}

function readInput(args) {
  const fileIndex = args.indexOf("--file");
  if (args.includes("--help") || args.includes("-h")) {
    process.stdout.write(`${usage()}\n`);
    process.exit(0);
  }
  if (fileIndex >= 0) {
    const file = args[fileIndex + 1];
    if (!file) {
      throw new Error("--file requires a path");
    }
    return fs.readFileSync(file, "utf8");
  }
  return fs.readFileSync(0, "utf8");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const classification = classifyGatePaths(readInput(process.argv.slice(2)));
    process.stdout.write(`${JSON.stringify(classification, null, 2)}\n`);
  } catch (error) {
    process.stderr.write(`gate-path-classifier: ${error.message}\n`);
    process.exit(2);
  }
}
