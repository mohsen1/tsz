---
title: Benchmarks
layout: layouts/base.njk
page_class: benchmarks
permalink: /benchmarks/index.html
---

# Benchmarks

R0 does not publish a rewrite performance dashboard. The committed pre-reset
snapshot and images are frozen historical artifacts; they are not current
compiler results.

## Publication rule

A timing is eligible for a public speed claim only after the same row is green:

- the real dependency graph is present;
- `tsz` and the pinned TypeScript `7.0.2` oracle agree on the project result;
- the run records binary, fixture, oracle, diagnostic, timing, and memory
  provenance;
- a failure, timeout, unsupported surface, or stubbed dependency cannot become a
  win by being fast.

The eventual target is at least 3x the throughput of `tsgo` on every green row.
R0 currently has no rows eligible for that claim.

## Retained harness

The project, microbenchmark, and performance runners remain available for local
engineering and observational CI artifacts:

```sh
./scripts/bench/bench-vs-tsgo.sh --json
```

During the rewrite, those results should expose unsupported and failed rows
rather than filter them away. A live dashboard returns when current rewrite
artifacts satisfy the publication rule above.
